//! Waiting without freezing (household rules 40–43), without libcosmic.
//!
//! Anything slow runs as a background wait started through a [`Waits`]
//! table. Starting one gives an [`Events`] stream: the app's glue turns it
//! into a task whose messages come back to `update` (with libcosmic,
//! `Task::stream(events.map(Message::Opened))`). The stream carries the
//! wait's tier changes (a spinner may show from 100 ms, a progress bar from
//! 1 s), its progress, a stall, and one ending. Past its deadline a wait
//! stops and comes back as a plain-words [`Problem`] instead of hanging.
//! `update` hands every [`Event`] to [`Waits::apply`], which keeps what the
//! view draws and passes on only results still wanted; `view` reads the
//! table ([`Waits::get`], [`Waits::showing`]) and draws the app's own
//! spinner, bar and Cancel button.
//!
//! ```ignore
//! Message::Open(path) => {
//!     let spec = Spec::new(Kind::Local, "Opening the document").key(key("open", &path));
//!     let (_, events) = self.waits.start_blocking(spec, move |_| read(&path));
//!     Task::stream(events.map(Message::Opened))
//! }
//! Message::Opened(event) => match self.waits.apply(event) {
//!     Some(Outcome::Done(doc)) => self.doc = doc,
//!     Some(Outcome::Failed(problem) | Outcome::TimedOut(problem)) => self.show(problem),
//!     None => {}
//! }
//! ```
//!
//! From the `ui_wait` draft (perf audit, 15 Oct), with the deadlines Tom and
//! Juliet agreed on 16 Oct. Needs a tokio runtime (libcosmic's executor is
//! one).

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::pin::Pin;
use std::process::{Child, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{Stream, StreamExt, stream};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant as TokioInstant, sleep_until};
pub use tokio_util::sync::CancellationToken;

/// When a wait gets a spinner (rule 41)
pub const SPINNER_AFTER: Duration = Duration::from_millis(100);
/// When a wait gets a progress bar (rule 41)
pub const BAR_AFTER: Duration = Duration::from_secs(1);
/// How long a wait showing a bar may go without progress before it says so
pub const STALL_AFTER: Duration = Duration::from_secs(15);
/// The deadline of work on this machine or the LAN
pub const LOCAL_DEADLINE: Duration = Duration::from_secs(2);
/// The deadline of work over ssh or the internet
pub const REMOTE_DEADLINE: Duration = Duration::from_secs(10);
/// The most work [`time_slice`] does on the UI thread at once (rule 43)
pub const SLICE: Duration = Duration::from_millis(4);
/// Progress is passed on at most this often, the latest report winning
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// What a wait that overran its deadline says, unless its spec says more
pub const TOO_LONG: &str = "This took too long and was stopped. Try again.";
/// What a stalled wait's bar says beneath it
pub const STILL_WAITING: &str = "Still waiting. Nothing has changed for 15 seconds.";

/// One wait, unique within its [`Waits`]. A later wait has a larger id, so
/// an id is also a generation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WaitId(u64);

/// What a wait is for: a kind of action and its subject (a path, a channel,
/// a provider). One wait per key is live at a time
pub type Key = (&'static str, u64);

/// A [`Key`] for `subject`, hashed
pub fn key(kind: &'static str, subject: &impl Hash) -> Key {
    let mut hasher = DefaultHasher::new();
    subject.hash(&mut hasher);
    (kind, hasher.finish())
}

/// Where a wait's work happens, which sets its default deadline (Tom and
/// Juliet, 16 Oct). The tier it shows goes by the time it actually takes,
/// whatever its kind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On this machine or the LAN (a file, a share on a home server, a
    /// settings write): stopped after [`LOCAL_DEADLINE`], 2 s
    Local,
    /// Over ssh or the internet (another machine's helper, a web request):
    /// stopped after [`REMOTE_DEADLINE`], 10 s
    Remote,
    /// Long work that reports progress (a scan, a copy, a setup chain): no
    /// total deadline, but flagged as stalled after [`STALL_AFTER`], 15 s,
    /// without progress. Cancellable by default, as nothing else ends it
    Long,
    /// Waiting for the person, not the computer (a QR scan, a portal, an
    /// incoming call): no deadline, no tiers, shown at once
    Person,
}

impl Kind {
    /// The deadline a wait of this kind has unless its [`Spec`] gives one
    pub fn default_deadline(self) -> Option<Duration> {
        match self {
            Kind::Local => Some(LOCAL_DEADLINE),
            Kind::Remote => Some(REMOTE_DEADLINE),
            Kind::Long | Kind::Person => None,
        }
    }

    /// The most a wait of this kind should show: a bar on short work means
    /// the kind or the code is wrong
    fn expected(self) -> Tier {
        match self {
            Kind::Local | Kind::Remote => Tier::Spinner,
            Kind::Long => Tier::Bar,
            Kind::Person => Tier::Person,
        }
    }
}

/// What the view shows for a wait now (rule 41)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Its first 100 ms: nothing
    Instant,
    /// From 100 ms: a 16 px spinner and its label
    Spinner,
    /// From 1 s: its label, a bar (with a fraction when the total is
    /// known), a caption ([`WaitView::caption`]) and Cancel if it can be
    /// cancelled
    Bar,
    /// A [`Kind::Person`] wait: its label and Cancel, at once
    Person,
}

/// A wait as it is started
#[derive(Clone, Debug)]
pub struct Spec {
    kind: Kind,
    label: String,
    deadline: Option<Duration>,
    key: Option<Key>,
    join: bool,
    cancellable: bool,
    too_long: String,
}

impl Spec {
    /// A wait of `kind` described by `label` ("Opening the document"),
    /// which the spinner and the bar show
    pub fn new(kind: Kind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            deadline: kind.default_deadline(),
            key: None,
            join: false,
            cancellable: matches!(kind, Kind::Person | Kind::Long),
            too_long: TOO_LONG.into(),
        }
    }

    /// Stop after `deadline` instead of the kind's default (a total
    /// deadline for long work too)
    pub fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// One live wait per `key`: starting another stops this one, and its
    /// result is dropped (latest wins)
    pub fn key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }

    /// With a key: while a wait for it is running, starting another joins
    /// it instead of replacing it (a Refresh during a refresh)
    pub fn join(mut self) -> Self {
        self.join = true;
        self
    }

    /// The person may stop it: the bar shows Cancel
    pub fn cancellable(mut self) -> Self {
        self.cancellable = true;
        self
    }

    /// The sentence shown when it overruns its deadline, saying what
    /// happened and what to do (rule 20): "Could not reach Claude. Try
    /// again in a minute."
    pub fn too_long(mut self, sentence: impl Into<String>) -> Self {
        self.too_long = sentence.into();
        self
    }
}

/// How far a long wait has got. With no total the bar moves without a
/// fraction
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
    /// What is happening now: "Reading tags", a file name
    pub detail: String,
}

impl Progress {
    pub fn fraction(&self) -> Option<f32> {
        self.total
            .filter(|&t| t > 0)
            .map(|t| (self.done as f32 / t as f32).clamp(0.0, 1.0))
    }
}

/// A failure in plain words (rule 20): `what` happened and what to do, for
/// the person; `detail` for Show details, Copy details and the log
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub what: String,
    pub detail: String,
}

impl Problem {
    pub fn new(what: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self {
            what: what.into(),
            detail: detail.to_string(),
        }
    }
}

/// What a wait's stream sends, for the app to wrap in its message
#[derive(Clone, Debug)]
pub enum Event<T> {
    Promote(WaitId, Tier),
    Progress(WaitId, Progress),
    /// No progress for [`STALL_AFTER`] while showing a bar
    Stalled(WaitId),
    Done(WaitId, T),
    Failed(WaitId, Problem),
    TimedOut(WaitId),
}

impl<T> Event<T> {
    pub fn id(&self) -> WaitId {
        match self {
            Event::Promote(id, _)
            | Event::Progress(id, _)
            | Event::Stalled(id)
            | Event::Done(id, _)
            | Event::Failed(id, _)
            | Event::TimedOut(id) => *id,
        }
    }
}

/// A wait's events, for the app to run as a task. Dropping it stops the
/// wait's work
pub type Events<T> = Pin<Box<dyn Stream<Item = Event<T>> + Send>>;

/// How a wait ended, as [`Waits::apply`] passes it on
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome<T> {
    Done(T),
    Failed(Problem),
    /// Its deadline passed; the problem carries the spec's sentence
    TimedOut(Problem),
}

/// What the work of a wait is handed: its cancellation token, a way to
/// report progress, and a register of child processes to kill if it stops
#[derive(Clone)]
pub struct Ctx {
    cancel: CancellationToken,
    progress: Arc<watch::Sender<Option<Progress>>>,
    children: Arc<Mutex<Vec<Arc<Mutex<Child>>>>>,
}

impl Ctx {
    fn new() -> (Self, watch::Receiver<Option<Progress>>) {
        let (tx, rx) = watch::channel(None);
        (
            Self {
                cancel: CancellationToken::new(),
                progress: Arc::new(tx),
                children: Default::default(),
            },
            rx,
        )
    }

    /// Whether the wait was cancelled, replaced or overran: blocking work
    /// looks between steps. (A future is dropped at once instead)
    pub fn cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// The wait's token, for work that hands it on
    pub fn token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Report progress. Cheap: reports closer than 100 ms collapse into the
    /// latest
    pub fn progress(&self, done: u64, total: Option<u64>, detail: impl Into<String>) {
        self.progress.send_replace(Some(Progress {
            done,
            total,
            detail: detail.into(),
        }));
    }

    /// Hand a child process to the wait, so it is killed if the wait is
    /// cancelled or overruns. (A `tokio::process` child in async work
    /// needs only `kill_on_drop(true)`.)
    pub fn own(&self, child: Child) -> OwnedChild {
        let child = Arc::new(Mutex::new(child));
        self.children.lock().unwrap().push(child.clone());
        OwnedChild {
            child,
            cancel: self.cancel.clone(),
        }
    }

    fn kill_children(&self) {
        for child in self.children.lock().unwrap().drain(..) {
            let mut c = child.lock().unwrap();
            let _ = c.kill();
            if !matches!(c.try_wait(), Ok(Some(_))) {
                drop(c);
                // Reaped on a blocking thread, never on the UI thread
                tokio::task::spawn_blocking(move || child.lock().unwrap().wait());
            }
        }
    }
}

/// A child process owned by a wait
pub struct OwnedChild {
    child: Arc<Mutex<Child>>,
    cancel: CancellationToken,
}

impl OwnedChild {
    /// Wait for it to exit, checking every 20 ms; an error if the wait was
    /// cancelled or overran (by then the process has been killed)
    pub fn wait(&self) -> std::io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.child.lock().unwrap().try_wait()? {
                return Ok(status);
            }
            if self.cancel.is_cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "the wait was stopped",
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The process itself, for its pipes and id
    pub fn with<R>(&self, f: impl FnOnce(&mut Child) -> R) -> R {
        f(&mut self.child.lock().unwrap())
    }
}

/// The time, real or a test's, for records and [`time_slice`]. (A wait's
/// own timers are tokio's, which tests pause)
#[derive(Clone)]
pub enum Clock {
    Real,
    Fake(Arc<Mutex<Instant>>),
}

impl Clock {
    pub fn fake() -> Self {
        Clock::Fake(Arc::new(Mutex::new(Instant::now())))
    }

    pub fn now(&self) -> Instant {
        match self {
            Clock::Real => Instant::now(),
            Clock::Fake(now) => *now.lock().unwrap(),
        }
    }

    /// Move a fake clock on; a real one is left alone
    pub fn advance(&self, by: Duration) {
        if let Clock::Fake(now) = self {
            *now.lock().unwrap() += by;
        }
    }
}

/// How one wait went, kept when [`Waits::recording`] is on (rule 43)
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub label: String,
    pub kind: Kind,
    /// The most it showed
    pub shown: Tier,
    pub took: Duration,
    pub end: End,
}

impl Record {
    /// It showed more than its kind should: a bar on local or remote work
    pub fn overran(&self) -> bool {
        self.kind != Kind::Person && self.shown > self.kind.expected()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Done,
    Failed,
    TimedOut,
    Cancelled,
    Replaced,
}

struct Live {
    spec: Spec,
    started: Instant,
    shown: Tier,
    progress: Option<Progress>,
    stalled: bool,
    cancel: CancellationToken,
}

/// A live wait as the view sees it
#[derive(Clone, Debug, PartialEq)]
pub struct WaitView<'a> {
    pub id: WaitId,
    pub label: &'a str,
    pub shown: Tier,
    pub progress: Option<&'a Progress>,
    pub stalled: bool,
    pub cancellable: bool,
}

impl WaitView<'_> {
    /// The line under a bar: [`STILL_WAITING`] when stalled, else "12 of
    /// 300: Album", "12 of 300" or what is happening; `None` when there is
    /// nothing to say or no bar
    pub fn caption(&self) -> Option<String> {
        if self.shown != Tier::Bar {
            return None;
        }
        if self.stalled {
            return Some(STILL_WAITING.into());
        }
        let caption = match self.progress? {
            Progress {
                done,
                total: Some(total),
                detail,
            } if detail.is_empty() => format!("{done} of {total}"),
            Progress {
                done,
                total: Some(total),
                detail,
            } => format!("{done} of {total}: {detail}"),
            Progress { detail, .. } => detail.clone(),
        };
        (!caption.is_empty()).then_some(caption)
    }

    /// The bar's fraction, if its total is known (else the bar moves
    /// without one)
    pub fn fraction(&self) -> Option<f32> {
        self.progress.and_then(Progress::fraction)
    }
}

/// The app's live waits: one field in the app, changed by `update`, read
/// by `view`
pub struct Waits {
    next: u64,
    live: HashMap<WaitId, Live>,
    keys: HashMap<Key, WaitId>,
    clock: Clock,
    records: Option<Vec<Record>>,
}

impl Default for Waits {
    fn default() -> Self {
        Self::with_clock(Clock::Real)
    }
}

impl Waits {
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            next: 0,
            live: HashMap::new(),
            keys: HashMap::new(),
            clock,
            records: None,
        }
    }

    /// Keep a [`Record`] of every wait, for tests and timing runs
    pub fn recording(mut self) -> Self {
        self.records = Some(Vec::new());
        self
    }

    /// The records so far, emptied
    pub fn take_records(&mut self) -> Vec<Record> {
        self.records
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Start async `work` under `spec`. Run the events as a task from
    /// `update`. A joined wait ([`Spec::join`]) gives the running wait's id
    /// and an empty stream
    pub fn start<T, F>(&mut self, spec: Spec, work: impl FnOnce(Ctx) -> F) -> (WaitId, Events<T>)
    where
        F: Future<Output = Result<T, Problem>> + Send + 'static,
        T: Send + 'static,
    {
        if let Some(id) = self.joinable(&spec) {
            return (id, stream::empty().boxed());
        }
        let (id, ctx, rx) = self.register(spec.clone());
        let future = work(ctx.clone());
        (id, drive(id, spec.kind, spec.deadline, ctx, rx, future))
    }

    /// Start blocking `work` (std::fs on a user path, a child process, a
    /// keyring call, PNG encoding) on tokio's blocking pool, as [`start`]
    /// does. A thread still stuck at the deadline is left to finish on its
    /// own; its result is dropped
    ///
    /// [`start`]: Waits::start
    pub fn start_blocking<T>(
        &mut self,
        spec: Spec,
        work: impl FnOnce(Ctx) -> Result<T, Problem> + Send + 'static,
    ) -> (WaitId, Events<T>)
    where
        T: Send + 'static,
    {
        self.start(spec, move |ctx| async move {
            tokio::task::spawn_blocking(move || work(ctx))
                .await
                .unwrap_or_else(|error| {
                    Err(Problem::new("Something went wrong. Try again.", error))
                })
        })
    }

    fn joinable(&self, spec: &Spec) -> Option<WaitId> {
        spec.join.then_some(())?;
        self.keys.get(spec.key.as_ref()?).copied()
    }

    fn register(&mut self, spec: Spec) -> (WaitId, Ctx, watch::Receiver<Option<Progress>>) {
        self.next += 1;
        let id = WaitId(self.next);
        if let Some(old) = spec.key.and_then(|key| self.keys.insert(key, id)) {
            self.end(old, End::Replaced);
        }
        let (ctx, rx) = Ctx::new();
        let shown = if spec.kind == Kind::Person {
            Tier::Person
        } else {
            Tier::Instant
        };
        let live = Live {
            spec,
            started: self.clock.now(),
            shown,
            progress: None,
            stalled: false,
            cancel: ctx.cancel.clone(),
        };
        self.live.insert(id, live);
        (id, ctx, rx)
    }

    /// Take in an event from a wait's stream. Returns how it ended if it
    /// ended and is still wanted; events of cancelled or replaced waits
    /// are dropped
    pub fn apply<T>(&mut self, event: Event<T>) -> Option<Outcome<T>> {
        let id = event.id();
        let live = self.live.get_mut(&id)?;
        match event {
            Event::Promote(_, shown) => live.shown = shown,
            Event::Progress(_, progress) => {
                live.progress = Some(progress);
                live.stalled = false;
            }
            Event::Stalled(_) => live.stalled = true,
            Event::Done(_, value) => {
                self.end(id, End::Done);
                return Some(Outcome::Done(value));
            }
            Event::Failed(_, problem) => {
                self.end(id, End::Failed);
                return Some(Outcome::Failed(problem));
            }
            Event::TimedOut(_) => {
                let detail = format!(
                    "{} did not finish within {:?}",
                    live.spec.label,
                    live.spec.deadline.unwrap_or_default()
                );
                let problem = Problem::new(live.spec.too_long.clone(), detail);
                self.end(id, End::TimedOut);
                return Some(Outcome::TimedOut(problem));
            }
        }
        None
    }

    /// Stop a wait (its Cancel button, a channel switch). Its stream kills
    /// its owned processes and ends; nothing more comes back from it
    pub fn cancel(&mut self, id: WaitId) {
        self.end(id, End::Cancelled);
    }

    /// Stop the live wait for `key`, if any
    pub fn cancel_key(&mut self, key: &Key) {
        if let Some(id) = self.keys.get(key).copied() {
            self.cancel(id);
        }
    }

    fn end(&mut self, id: WaitId, end: End) {
        let Some(live) = self.live.remove(&id) else {
            return;
        };
        if matches!(end, End::Cancelled | End::Replaced) {
            live.cancel.cancel();
        }
        if let Some(key) = live.spec.key
            && self.keys.get(&key) == Some(&id)
        {
            self.keys.remove(&key);
        }
        if let Some(records) = &mut self.records {
            let took = self.clock.now().saturating_duration_since(live.started);
            records.push(Record {
                label: live.spec.label,
                kind: live.spec.kind,
                shown: live.shown,
                took,
                end,
            });
        }
    }

    pub fn get(&self, id: WaitId) -> Option<WaitView<'_>> {
        let live = self.live.get(&id)?;
        Some(WaitView {
            id,
            label: &live.spec.label,
            shown: live.shown,
            progress: live.progress.as_ref(),
            stalled: live.stalled,
            cancellable: live.spec.cancellable,
        })
    }

    /// The live wait for `key`
    pub fn for_key(&self, key: &Key) -> Option<WaitView<'_>> {
        self.get(*self.keys.get(key)?)
    }

    pub fn is_live(&self, id: WaitId) -> bool {
        self.live.contains_key(&id)
    }

    /// Live waits showing something, oldest first, for a shared activity
    /// area
    pub fn showing(&self) -> Vec<WaitView<'_>> {
        let mut ids: Vec<WaitId> = self
            .live
            .iter()
            .filter(|(_, l)| l.shown != Tier::Instant)
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids.into_iter().filter_map(|id| self.get(id)).collect()
    }
}

/// Never finishes when `at` is None
async fn until(at: Option<TokioInstant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The stream of one wait: runs `work` against its deadline and token and
/// sends tiers, progress and the result as events
fn drive<T: Send + 'static>(
    id: WaitId,
    kind: Kind,
    deadline: Option<Duration>,
    ctx: Ctx,
    mut rx: watch::Receiver<Option<Progress>>,
    work: impl Future<Output = Result<T, Problem>> + Send + 'static,
) -> Events<T> {
    let (out, mut events) = mpsc::channel::<Event<T>>(4);
    let runner = async move {
        let start = TokioInstant::now();
        let end_at = deadline.map(|d| start + d);
        let mut work = std::pin::pin!(work);
        let mut shown = if kind == Kind::Person {
            Tier::Person
        } else {
            Tier::Instant
        };
        let mut last_progress = start;
        let mut stalled = false;
        let mut progress_open = true;
        let mut progress_ready = start;
        loop {
            let promote_at = match shown {
                Tier::Instant => Some(start + SPINNER_AFTER),
                Tier::Spinner => Some(start + BAR_AFTER),
                Tier::Bar | Tier::Person => None,
            };
            let stall_at = (shown == Tier::Bar && !stalled).then(|| last_progress + STALL_AFTER);
            let throttled = progress_ready > TokioInstant::now();
            let event = tokio::select! {
                biased;
                _ = ctx.cancel.cancelled() => {
                    ctx.kill_children();
                    return;
                }
                result = &mut work => match result {
                    Ok(value) => Event::Done(id, value),
                    Err(problem) => Event::Failed(id, problem),
                },
                _ = until(end_at) => {
                    ctx.cancel.cancel();
                    ctx.kill_children();
                    let _ = out.send(Event::TimedOut(id)).await;
                    return;
                }
                _ = until(promote_at) => {
                    shown = if shown == Tier::Instant { Tier::Spinner } else { Tier::Bar };
                    Event::Promote(id, shown)
                }
                _ = until(stall_at) => {
                    stalled = true;
                    Event::Stalled(id)
                }
                _ = until(Some(progress_ready)), if throttled => continue,
                changed = rx.changed(), if progress_open && !throttled => {
                    let Ok(()) = changed else { progress_open = false; continue };
                    let Some(progress) = rx.borrow_and_update().clone() else { continue };
                    last_progress = TokioInstant::now();
                    progress_ready = last_progress + PROGRESS_EVERY;
                    stalled = false;
                    Event::Progress(id, progress)
                }
            };
            let finished = matches!(event, Event::Done(..) | Event::Failed(..));
            if out.send(event).await.is_err() || finished {
                return;
            }
        }
    };
    // The runner is polled as part of the stream, so dropping the stream
    // drops the work; the stream ends when the runner has ended and its
    // events are read
    let received = stream::poll_fn(move |cx| events.poll_recv(cx));
    stream::select(
        received,
        stream::once(runner).filter_map(|()| async { None }),
    )
    .boxed()
}

/// Results tagged by generation, for work not started through [`Waits`]
/// (a revision-tagged re-parse): the latest generation per key wins
#[derive(Default)]
pub struct Latest<K: Hash + Eq> {
    generations: HashMap<K, u64>,
}

impl<K: Hash + Eq> Latest<K> {
    /// A new generation for `key`; earlier ones are now stale
    pub fn next(&mut self, key: K) -> u64 {
        let generation = self.generations.entry(key).or_default();
        *generation += 1;
        *generation
    }

    pub fn is_current(&self, key: &K, generation: u64) -> bool {
        self.generations.get(key) == Some(&generation)
    }
}

/// Acts once changes have stopped for a while (typing in Settings, a
/// volume drag)
#[derive(Default)]
pub struct Debounce {
    generation: u64,
}

impl Debounce {
    /// A change. The future gives its generation after `quiet`: run it as
    /// a task, and act only if [`Debounce::is_latest`] says no change came
    /// since
    pub fn poke(&mut self, quiet: Duration) -> impl Future<Output = u64> + Send + use<> {
        self.generation += 1;
        let generation = self.generation;
        async move {
            tokio::time::sleep(quiet).await;
            generation
        }
    }

    pub fn is_latest(&self, generation: u64) -> bool {
        generation == self.generation
    }
}

/// Gathers many small events (scan results) into one message per interval
pub struct Coalesce<T> {
    items: Vec<T>,
}

impl<T> Default for Coalesce<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Coalesce<T> {
    /// Add an item. The first of a batch gives a timer that finishes after
    /// `every`: run it as a task whose message [`Coalesce::take`]s the batch
    pub fn push(&mut self, item: T, every: Duration) -> Option<tokio::time::Sleep> {
        self.items.push(item);
        (self.items.len() == 1).then(|| tokio::time::sleep(every))
    }

    pub fn take(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
    }
}

/// Jobs that must run one at a time and in order per key, as saves to one
/// path. With `collapse`, a job waiting behind a running one is replaced
/// by a newer one (only the latest content needs saving)
pub struct Queue<K: Hash + Eq + Clone, J> {
    keys: HashMap<K, VecDeque<J>>,
    collapse: bool,
}

impl<K: Hash + Eq + Clone, J> Queue<K, J> {
    pub fn new(collapse: bool) -> Self {
        Self {
            keys: HashMap::new(),
            collapse,
        }
    }

    /// Returns the job if it can start now; otherwise it waits its turn
    pub fn push(&mut self, key: K, job: J) -> Option<J> {
        match self.keys.get_mut(&key) {
            None => {
                self.keys.insert(key, VecDeque::new());
                Some(job)
            }
            Some(waiting) => {
                if self.collapse {
                    waiting.clear();
                }
                waiting.push_back(job);
                None
            }
        }
    }

    /// The running job for `key` has ended; returns the next to start
    pub fn finished(&mut self, key: &K) -> Option<J> {
        let waiting = self.keys.get_mut(key)?;
        let next = waiting.pop_front();
        if next.is_none() {
            self.keys.remove(key);
        }
        next
    }

    /// Nothing running or waiting (safe to quit)
    pub fn is_idle(&self) -> bool {
        self.keys.is_empty()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Sliced {
    Done,
    /// Time is up: send a message to carry on after the next frame
    More,
}

/// Run `step` until it says there is no more (returns false) or `budget`
/// has gone, for work that must stay on the UI thread in pieces
pub fn time_slice(clock: &Clock, budget: Duration, mut step: impl FnMut() -> bool) -> Sliced {
    let start = clock.now();
    while step() {
        if clock.now().duration_since(start) >= budget {
            return Sliced::More;
        }
    }
    Sliced::Done
}

/// How long `update` and `view` take, per message kind (rule 43)
#[derive(Default)]
pub struct Timings {
    by_label: HashMap<&'static str, (u32, Duration)>,
}

impl Timings {
    pub fn time<R>(&mut self, label: &'static str, f: impl FnOnce() -> R) -> R {
        let start = Instant::now();
        let result = f();
        let took = start.elapsed();
        let (count, max) = self.by_label.entry(label).or_default();
        *count += 1;
        *max = (*max).max(took);
        result
    }

    /// The labels whose longest run went over `budget`, longest first
    pub fn over(&self, budget: Duration) -> Vec<(&'static str, Duration)> {
        let mut over: Vec<_> = self
            .by_label
            .iter()
            .filter(|(_, (_, max))| *max > budget)
            .map(|(l, (_, max))| (*l, *max))
            .collect();
        over.sort_by_key(|&(_, max)| std::cmp::Reverse(max));
        over
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run<T>(events: Events<T>) -> Vec<Event<T>> {
        events.collect().await
    }

    fn sleeper(
        after: Duration,
        value: u32,
    ) -> impl FnOnce(Ctx) -> Pin<Box<dyn Future<Output = Result<u32, Problem>> + Send>> {
        move |_| {
            Box::pin(async move {
                tokio::time::sleep(after).await;
                Ok(value)
            })
        }
    }

    fn tiers(events: &[Event<u32>]) -> Vec<Tier> {
        events
            .iter()
            .filter_map(|e| {
                if let Event::Promote(_, s) = e {
                    Some(*s)
                } else {
                    None
                }
            })
            .collect()
    }

    fn outcome(waits: &mut Waits, events: Vec<Event<u32>>) -> Option<Outcome<u32>> {
        events.into_iter().filter_map(|e| waits.apply(e)).last()
    }

    #[tokio::test(start_paused = true)]
    async fn quick_work_shows_nothing() {
        let mut waits = Waits::default();
        let (_, events) = waits.start(
            Spec::new(Kind::Local, "Saving"),
            sleeper(Duration::from_millis(60), 1),
        );
        let events = run(events).await;
        assert!(tiers(&events).is_empty());
        assert!(matches!(events.last(), Some(Event::Done(_, 1))));
    }

    #[tokio::test(start_paused = true)]
    async fn promotes_at_100_ms_and_1_s() {
        let mut waits = Waits::with_clock(Clock::fake()).recording();
        let (id, events) = waits.start(
            Spec::new(Kind::Remote, "Loading"),
            sleeper(Duration::from_millis(1500), 7),
        );
        let started = TokioInstant::now();
        let mut events = events;
        let mut seen = Vec::new();
        while let Some(event) = events.next().await {
            if let Event::Promote(_, tier) = event {
                seen.push((tier, started.elapsed()));
                waits.apply(event);
                assert_eq!(waits.get(id).unwrap().shown, tier);
            } else {
                assert_eq!(waits.apply(event), Some(Outcome::Done(7)));
            }
        }
        assert_eq!(
            seen,
            [(Tier::Spinner, SPINNER_AFTER), (Tier::Bar, BAR_AFTER)]
        );
        let records = waits.take_records();
        assert_eq!(records[0].shown, Tier::Bar);
        assert!(
            records[0].overran(),
            "remote work that showed a bar is an overrun"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn the_agreed_deadlines() {
        assert_eq!(Kind::Local.default_deadline(), Some(Duration::from_secs(2)));
        assert_eq!(
            Kind::Remote.default_deadline(),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            (
                Kind::Long.default_deadline(),
                Kind::Person.default_deadline()
            ),
            (None, None)
        );
        for (kind, deadline) in [(Kind::Local, 2), (Kind::Remote, 10)] {
            let mut waits = Waits::default();
            let (_, events) = waits.start(
                Spec::new(kind, "Reading"),
                sleeper(Duration::from_secs(60), 3),
            );
            let started = TokioInstant::now();
            let events = run(events).await;
            assert_eq!(started.elapsed(), Duration::from_secs(deadline), "{kind:?}");
            let Some(Outcome::TimedOut(problem)) = outcome(&mut waits, events) else {
                panic!("{kind:?} did not time out")
            };
            assert_eq!(
                problem.what,
                "This took too long and was stopped. Try again."
            );
            assert_eq!(
                problem.detail,
                format!("Reading did not finish within {deadline}s")
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_deadline_of_its_own_and_a_sentence_of_its_own() {
        let mut waits = Waits::default();
        let spec = Spec::new(Kind::Remote, "Loading messages")
            .deadline(Duration::from_secs(3))
            .too_long("Could not load the messages. Try again.");
        let (_, events) = waits.start(spec, sleeper(Duration::from_secs(60), 3));
        let started = TokioInstant::now();
        let events = run(events).await;
        assert_eq!(started.elapsed(), Duration::from_secs(3));
        let Some(Outcome::TimedOut(problem)) = outcome(&mut waits, events) else {
            panic!()
        };
        assert_eq!(problem.what, "Could not load the messages. Try again.");
    }

    #[tokio::test(start_paused = true)]
    async fn person_waits_never_promote_or_time_out() {
        let mut waits = Waits::default();
        let (id, events) = waits.start(
            Spec::new(Kind::Person, "Scan the code"),
            sleeper(Duration::from_secs(3600), 2),
        );
        assert_eq!(
            waits.get(id).map(|w| (w.shown, w.cancellable)),
            Some((Tier::Person, true))
        );
        let events = run(events).await;
        assert!(tiers(&events).is_empty());
        assert!(matches!(events.last(), Some(Event::Done(_, 2))));
    }

    #[tokio::test(start_paused = true)]
    async fn long_work_has_no_deadline_and_says_when_it_stalls() {
        let mut waits = Waits::default();
        let spec = Spec::new(Kind::Long, "Scanning the collection");
        let (id, events) = waits.start(spec, |ctx| async move {
            for n in 1..=3 {
                tokio::time::sleep(Duration::from_millis(400)).await;
                ctx.progress(n, Some(3), format!("Album {n}"));
            }
            // A minute with nothing new: stalled, never stopped
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(0u32)
        });
        let started = TokioInstant::now();
        let mut events = events;
        let mut stalled_at = None;
        let mut progress = Vec::new();
        while let Some(event) = events.next().await {
            match &event {
                Event::Progress(_, p) => progress.push(p.done),
                Event::Stalled(_) => stalled_at = Some(started.elapsed()),
                Event::Done(..) => {
                    let view = waits.get(id).unwrap();
                    assert_eq!(
                        (view.shown, view.stalled, view.cancellable),
                        (Tier::Bar, true, true)
                    );
                    assert_eq!(view.fraction(), Some(1.0));
                    assert_eq!(
                        view.caption().as_deref(),
                        Some("Still waiting. Nothing has changed for 15 seconds.")
                    );
                }
                _ => {}
            }
            waits.apply(event);
        }
        assert_eq!(progress, [1, 2, 3]);
        // 15 s after the last progress, at 1.2 s
        assert_eq!(stalled_at, Some(Duration::from_millis(1200) + STALL_AFTER));
        assert_eq!(started.elapsed(), Duration::from_millis(61_200));
    }

    #[tokio::test(start_paused = true)]
    async fn progress_resumes_after_a_stall_and_bursts_collapse() {
        let mut waits = Waits::default();
        let (id, events) = waits.start(Spec::new(Kind::Long, "Copying"), |ctx| async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            // A burst inside 100 ms: only the latest is passed on
            for n in 1..=50 {
                ctx.progress(n, None, format!("file {n}"));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(1u32)
        });
        let mut events = events;
        let mut passed = Vec::new();
        let mut stalled = false;
        while let Some(event) = events.next().await {
            stalled |= matches!(event, Event::Stalled(_));
            if let Event::Progress(_, p) = &event {
                assert!(stalled, "stalled at 15 s, before the progress at 20 s");
                passed.push(p.done);
                waits.apply(event);
                let view = waits.get(id).unwrap();
                assert!(!view.stalled);
                assert_eq!(view.caption().as_deref(), Some("file 50"));
                assert_eq!(view.fraction(), None);
            } else {
                waits.apply(event);
            }
        }
        assert_eq!(passed, [50]);
    }

    #[tokio::test(start_paused = true)]
    async fn the_latest_wait_for_a_key_wins() {
        let mut waits = Waits::default().recording();
        let k = key("search", &"channel 1");
        let (first, old) = waits.start(
            Spec::new(Kind::Remote, "Searching").key(k),
            sleeper(Duration::from_millis(50), 1),
        );
        let (second, new) = waits.start(
            Spec::new(Kind::Remote, "Searching").key(k),
            sleeper(Duration::from_millis(50), 2),
        );
        assert!(!waits.is_live(first) && waits.is_live(second));
        assert_eq!(waits.for_key(&k).map(|w| w.id), Some(second));
        // The replaced wait was cancelled: its stream ends without a result
        assert!(run(old).await.is_empty());
        // A late result for it would be dropped all the same
        assert_eq!(waits.apply(Event::Done(first, 1)), None);
        assert_eq!(outcome(&mut waits, run(new).await), Some(Outcome::Done(2)));
        assert_eq!(waits.take_records()[0].end, End::Replaced);
    }

    #[tokio::test(start_paused = true)]
    async fn join_keeps_the_running_wait() {
        let mut waits = Waits::default();
        let k = key("refresh", &0);
        let (first, running) = waits.start(
            Spec::new(Kind::Remote, "Checking").key(k).join(),
            sleeper(Duration::from_secs(1), 1),
        );
        let (again, none) = waits.start(
            Spec::new(Kind::Remote, "Checking").key(k).join(),
            sleeper(Duration::from_secs(1), 2),
        );
        assert_eq!(first, again);
        assert!(run(none).await.is_empty());
        assert_eq!(
            outcome(&mut waits, run(running).await),
            Some(Outcome::Done(1))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cancelling_stops_the_work_and_its_token() {
        let mut waits = Waits::default().recording();
        let (seen_tx, mut seen_rx) = mpsc::unbounded_channel();
        let (id, events) = waits.start(Spec::new(Kind::Long, "Setting up"), move |ctx| {
            let token = ctx.token().clone();
            tokio::spawn(async move {
                token.cancelled().await;
                let _ = seen_tx.send(());
            });
            async move {
                tokio::time::sleep(Duration::from_secs(600)).await;
                Ok(0u32)
            }
        });
        let running = tokio::spawn(run(events));
        tokio::time::sleep(Duration::from_secs(2)).await;
        waits.cancel(id);
        let events = running.await.unwrap();
        assert!(
            events
                .iter()
                .all(|e| !matches!(e, Event::Done(..) | Event::Failed(..) | Event::TimedOut(..)))
        );
        assert_eq!(
            seen_rx.recv().await,
            Some(()),
            "work handed the token sees the cancel"
        );
        assert!(waits.get(id).is_none());
        assert_eq!(waits.take_records()[0].end, End::Cancelled);
    }

    #[tokio::test(start_paused = true)]
    async fn failures_and_the_showing_list() {
        let mut waits = Waits::default();
        let (failing, events) = waits.start(Spec::new(Kind::Local, "Saving"), |_| async {
            Err::<u32, _>(Problem::new("Could not save. Try again.", "disk full"))
        });
        assert_eq!(
            outcome(&mut waits, run(events).await),
            Some(Outcome::Failed(Problem::new(
                "Could not save. Try again.",
                "disk full"
            )))
        );
        assert!(!waits.is_live(failing));
        let (a, _ea) = waits.start(
            Spec::new(Kind::Person, "Approve"),
            sleeper(Duration::from_secs(9), 0),
        );
        let (_b, _eb) = waits.start(
            Spec::new(Kind::Local, "Quiet"),
            sleeper(Duration::from_secs(1), 0),
        );
        let showing: Vec<WaitId> = waits.showing().iter().map(|w| w.id).collect();
        assert_eq!(showing, [a], "only what shows something, oldest first");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_kills_an_owned_child() {
        let mut waits = Waits::default();
        let (pid_tx, pid_rx) = std::sync::mpsc::channel();
        let spec = Spec::new(Kind::Long, "Stopping the invitation");
        let (id, events) = waits.start_blocking(spec, move |ctx| {
            let child = ctx.own(
                std::process::Command::new("sleep")
                    .arg("30")
                    .spawn()
                    .map_err(|e| Problem::new("Could not start", e))?,
            );
            pid_tx.send(child.with(|c| c.id())).unwrap();
            child.wait().map_err(|e| Problem::new("Stopped", e))?;
            Ok(0u32)
        });
        let running = tokio::spawn(run(events));
        let pid = tokio::task::spawn_blocking(move || pid_rx.recv().unwrap())
            .await
            .unwrap();
        let started = Instant::now();
        waits.cancel(id);
        let events = tokio::time::timeout(Duration::from_secs(5), running)
            .await
            .expect("stream ended")
            .unwrap();
        assert!(
            events
                .iter()
                .all(|e| !matches!(e, Event::Done(..) | Event::Failed(..)))
        );
        // The process is gone (killed and reaped), well before its 30 s
        let gone = || !std::path::Path::new(&format!("/proc/{pid}")).exists();
        while !gone() && started.elapsed() < Duration::from_secs(5) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(gone(), "sleep 30 was not killed");
        assert!(waits.get(id).is_none());
    }

    #[test]
    fn captions() {
        let progress = |done, total, detail: &str| Progress {
            done,
            total,
            detail: detail.into(),
        };
        fn view(shown: Tier, progress: Option<&Progress>, stalled: bool) -> WaitView<'_> {
            WaitView {
                id: WaitId(1),
                label: "Scanning",
                shown,
                progress,
                stalled,
                cancellable: true,
            }
        }
        let p = progress(12, Some(300), "Album");
        assert_eq!(
            view(Tier::Bar, Some(&p), false).caption().as_deref(),
            Some("12 of 300: Album")
        );
        let p = progress(12, Some(300), "");
        assert_eq!(
            view(Tier::Bar, Some(&p), false).caption().as_deref(),
            Some("12 of 300")
        );
        assert_eq!(view(Tier::Bar, Some(&p), false).fraction(), Some(0.04));
        assert_eq!(view(Tier::Bar, None, false).caption(), None);
        assert_eq!(
            view(Tier::Bar, None, true).caption().as_deref(),
            Some(STILL_WAITING)
        );
        assert_eq!(
            view(Tier::Spinner, Some(&p), true).caption(),
            None,
            "no bar, no caption"
        );
        assert_eq!(
            STILL_WAITING,
            format!(
                "Still waiting. Nothing has changed for {} seconds.",
                STALL_AFTER.as_secs()
            )
        );
    }

    #[test]
    fn saves_to_one_path_run_in_order_and_collapse() {
        let mut saves = Queue::new(true);
        assert_eq!(saves.push("a.md", 1), Some(1));
        assert_eq!(
            saves.push("b.md", 1),
            Some(1),
            "another path is not held up"
        );
        assert_eq!(saves.push("a.md", 2), None);
        assert_eq!(saves.push("a.md", 3), None);
        assert_eq!(
            saves.finished(&"a.md"),
            Some(3),
            "only the latest waiting save runs"
        );
        assert_eq!(saves.finished(&"a.md"), None);
        assert_eq!(saves.finished(&"b.md"), None);
        assert!(saves.is_idle());

        let mut ordered = Queue::new(false);
        ordered.push("k", 1);
        ordered.push("k", 2);
        ordered.push("k", 3);
        assert_eq!(
            (
                ordered.finished(&"k"),
                ordered.finished(&"k"),
                ordered.finished(&"k")
            ),
            (Some(2), Some(3), None)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn debounce_and_coalesce() {
        let mut latest = Latest::default();
        let old = latest.next("preview");
        let new = latest.next("preview");
        assert!(!latest.is_current(&"preview", old) && latest.is_current(&"preview", new));

        let mut debounce = Debounce::default();
        let first = debounce.poke(Duration::from_millis(300));
        let second = debounce.poke(Duration::from_millis(300));
        let started = TokioInstant::now();
        let (a, b) = tokio::join!(first, second);
        assert_eq!(started.elapsed(), Duration::from_millis(300));
        assert!(!debounce.is_latest(a) && debounce.is_latest(b));

        let mut batch = Coalesce::default();
        let timer = batch
            .push(1, Duration::from_millis(250))
            .expect("the first starts the timer");
        assert!(batch.push(2, Duration::from_millis(250)).is_none());
        timer.await;
        assert_eq!(batch.take(), [1, 2]);
        assert!(
            batch.push(3, Duration::from_millis(250)).is_some(),
            "a new batch"
        );
    }

    #[test]
    fn time_slices_stop_at_the_budget() {
        let clock = Clock::fake();
        let mut left = 10;
        let step = |left: &mut i32| {
            clock.advance(Duration::from_millis(1));
            *left -= 1;
            *left > 0
        };
        assert_eq!(time_slice(&clock, SLICE, || step(&mut left)), Sliced::More);
        assert_eq!(left, 6);
        assert_eq!(time_slice(&clock, SLICE, || step(&mut left)), Sliced::More);
        assert_eq!(time_slice(&clock, SLICE, || step(&mut left)), Sliced::Done);
        assert_eq!(left, 0);
    }

    #[test]
    fn timings_report_what_went_over() {
        let mut timings = Timings::default();
        timings.time("quick", || ());
        timings.time("slow", || std::thread::sleep(Duration::from_millis(10)));
        let over = timings.over(Duration::from_millis(4));
        assert_eq!(over.iter().map(|(l, _)| *l).collect::<Vec<_>>(), ["slow"]);
    }
}
