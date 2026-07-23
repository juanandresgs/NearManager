//! Bounded cancellable task execution for Near runtimes.

use std::{
    collections::BTreeMap,
    future::Future,
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    task::{Context, Poll, Wake, Waker},
    thread::{self, JoinHandle},
};

use near_core::CancellationToken;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeTaskId(pub u64);

#[derive(Clone, Debug)]
pub struct TaskHandle {
    id: RuntimeTaskId,
    cancellation: CancellationToken,
}

impl TaskHandle {
    pub fn id(&self) -> RuntimeTaskId {
        self.id
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    pub completed: u64,
    pub total: Option<u64>,
    pub message: String,
}

impl TaskRecord {
    pub fn running(
        task: &TaskHandle,
        title: impl Into<String>,
        total: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: task.id().0.to_string(),
            title: title.into(),
            state: TaskState::Running,
            completed: 0,
            total,
            message: message.into(),
        }
    }

    pub fn finish(&mut self, success: bool, message: impl Into<String>) {
        self.state = if success {
            TaskState::Completed
        } else {
            TaskState::Failed
        };
        self.completed = u64::from(success);
        self.message = message.into();
    }

    pub fn cancel(&mut self, message: impl Into<String>) {
        self.state = TaskState::Cancelled;
        self.message = message.into();
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = TaskState::Failed;
        self.message = message.into();
    }
}

#[derive(Debug)]
pub enum TaskOutcome<T> {
    Completed(T),
    Cancelled,
    Panicked,
}

#[derive(Debug)]
pub struct TaskCompletion<T> {
    pub id: RuntimeTaskId,
    pub outcome: TaskOutcome<T>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SpawnError {
    #[error("task queue is full")]
    QueueFull,
    #[error("task runtime is shut down")]
    ShutDown,
}

type Job<T> = Box<dyn FnOnce(CancellationToken) -> T + Send + 'static>;
type CompletionWake = Arc<dyn Fn() + Send + Sync + 'static>;

#[derive(Clone)]
pub struct TaskWakeHandle {
    wake: Arc<Mutex<Option<CompletionWake>>>,
}

impl TaskWakeHandle {
    pub fn wake(&self) {
        if let Some(wake) = self
            .wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            wake();
        }
    }
}

enum Message<T> {
    Run {
        id: RuntimeTaskId,
        cancellation: CancellationToken,
        job: Job<T>,
    },
    Shutdown,
}

pub struct TaskPool<T: Send + 'static> {
    sender: SyncSender<Message<T>>,
    completions: Receiver<TaskCompletion<T>>,
    active: Arc<Mutex<BTreeMap<RuntimeTaskId, CancellationToken>>>,
    workers: Vec<JoinHandle<()>>,
    next_id: AtomicU64,
    completion_wake: Arc<Mutex<Option<CompletionWake>>>,
}

impl<T: Send + 'static> TaskPool<T> {
    /// Creates a bounded worker pool.
    ///
    /// # Panics
    ///
    /// Panics when `workers` or `queue_capacity` is zero, or when an operating-system worker thread
    /// cannot be spawned.
    pub fn new(workers: usize, queue_capacity: usize) -> Self {
        assert!(workers > 0, "task runtime requires at least one worker");
        assert!(
            queue_capacity > 0,
            "task runtime queue must be bounded above zero"
        );
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let (completion_sender, completions) = mpsc::channel();
        let active = Arc::new(Mutex::new(BTreeMap::new()));
        let completion_wake = Arc::new(Mutex::new(None));
        let workers = (0..workers)
            .map(|index| {
                let receiver = Arc::clone(&receiver);
                let completion_sender = completion_sender.clone();
                let active = Arc::clone(&active);
                let completion_wake = Arc::clone(&completion_wake);
                thread::Builder::new()
                    .name(format!("near-task-{index}"))
                    .spawn(move || {
                        worker_loop(receiver, completion_sender, active, completion_wake);
                    })
                    .expect("Near task worker must spawn")
            })
            .collect();
        Self {
            sender,
            completions,
            active,
            workers,
            next_id: AtomicU64::new(1),
            completion_wake,
        }
    }

    /// Queues a cancellable job without blocking the caller.
    ///
    /// # Errors
    ///
    /// Returns `QueueFull` when bounded capacity is exhausted or `ShutDown` after runtime closure.
    pub fn spawn(
        &self,
        job: impl FnOnce(CancellationToken) -> T + Send + 'static,
    ) -> Result<TaskHandle, SpawnError> {
        let id = RuntimeTaskId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let cancellation = CancellationToken::default();
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, cancellation.clone());
        let message = Message::Run {
            id,
            cancellation: cancellation.clone(),
            job: Box::new(job),
        };
        match self.sender.try_send(message) {
            Ok(()) => Ok(TaskHandle { id, cancellation }),
            Err(TrySendError::Full(_)) => {
                self.remove_active(id);
                Err(SpawnError::QueueFull)
            }
            Err(TrySendError::Disconnected(_)) => {
                self.remove_active(id);
                Err(SpawnError::ShutDown)
            }
        }
    }

    pub fn try_completion(&self) -> Option<TaskCompletion<T>> {
        self.completions.try_recv().ok()
    }

    pub fn active_count(&self) -> usize {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    pub fn set_completion_wake(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self
            .completion_wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(wake));
    }

    pub fn wake_handle(&self) -> TaskWakeHandle {
        TaskWakeHandle {
            wake: Arc::clone(&self.completion_wake),
        }
    }

    pub fn clear_completion_wake(&self) {
        self.completion_wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    pub fn cancel_all(&self) {
        for cancellation in self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
        {
            cancellation.cancel();
        }
    }

    fn remove_active(&self, id: RuntimeTaskId) {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
    }
}

impl<T: Send + 'static> Drop for TaskPool<T> {
    fn drop(&mut self) {
        self.cancel_all();
        for _ in &self.workers {
            let _ = self.sender.send(Message::Shutdown);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
fn worker_loop<T: Send + 'static>(
    receiver: Arc<Mutex<Receiver<Message<T>>>>,
    completions: mpsc::Sender<TaskCompletion<T>>,
    active: Arc<Mutex<BTreeMap<RuntimeTaskId, CancellationToken>>>,
    completion_wake: Arc<Mutex<Option<CompletionWake>>>,
) {
    loop {
        let message = receiver
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(message) = message else {
            break;
        };
        let Message::Run {
            id,
            cancellation,
            job,
        } = message
        else {
            break;
        };
        let outcome = if cancellation.is_cancelled() {
            TaskOutcome::Cancelled
        } else {
            match catch_unwind(AssertUnwindSafe(|| job(cancellation.clone()))) {
                Ok(value) => TaskOutcome::Completed(value),
                Err(_) => TaskOutcome::Panicked,
            }
        };
        active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&id);
        let _ = completions.send(TaskCompletion { id, outcome });
        if let Some(wake) = completion_wake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            wake();
        }
    }
}

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

/// Blocks the current worker thread until a future completes without busy polling.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    block_on_pinned(future.as_mut())
}

fn block_on_pinned<F: Future + ?Sized>(mut future: Pin<&mut F>) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match Future::poll(future.as_mut(), &mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc, Condvar, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        task::Poll,
        time::{Duration, Instant},
    };

    use super::*;

    #[test]
    fn jobs_complete_and_panics_are_isolated() {
        let pool = TaskPool::new(2, 4);
        pool.spawn(|_| 42).unwrap();
        pool.spawn(|_| -> i32 { panic!("injected") }).unwrap();
        let mut completed = false;
        let mut panicked = false;
        for _ in 0..2 {
            let completion = pool
                .completions
                .recv_timeout(Duration::from_secs(5))
                .expect("both jobs must report a completion");
            match completion.outcome {
                TaskOutcome::Completed(42) => completed = true,
                TaskOutcome::Panicked => panicked = true,
                _ => {}
            }
        }
        assert!(completed && panicked);
    }

    #[test]
    fn completion_wake_notifies_blocked_event_loops() {
        let pool = TaskPool::new(1, 1);
        let notified = Arc::new((Mutex::new(false), Condvar::new()));
        let wake_state = Arc::clone(&notified);
        pool.set_completion_wake(move || {
            let (lock, condition) = &*wake_state;
            *lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
            condition.notify_one();
        });
        pool.spawn(|_| 42).unwrap();
        let (lock, condition) = &*notified;
        let (notified, timeout) = condition
            .wait_timeout_while(
                lock.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                Duration::from_secs(1),
                |notified| !*notified,
            )
            .unwrap();
        assert!(*notified);
        assert!(!timeout.timed_out());
        assert!(matches!(
            pool.try_completion().unwrap().outcome,
            TaskOutcome::Completed(42)
        ));
    }

    #[test]
    fn wake_handle_observes_callbacks_installed_after_handle_creation() {
        let pool = TaskPool::<()>::new(1, 1);
        let handle = pool.wake_handle();
        let notified = Arc::new(AtomicBool::new(false));
        let wake_state = Arc::clone(&notified);
        pool.set_completion_wake(move || wake_state.store(true, Ordering::Release));
        handle.wake();
        assert!(notified.load(Ordering::Acquire));
    }

    #[test]
    fn task_records_expose_running_completion_cancellation_and_failure() {
        let pool = TaskPool::<()>::new(1, 1);
        let task = pool.spawn(|_| {}).unwrap();
        let mut record = TaskRecord::running(&task, "Fixture", Some(1), "Running");
        assert_eq!(record.state, TaskState::Running);
        record.finish(true, "Done");
        assert_eq!((record.state, record.completed), (TaskState::Completed, 1));
        record.cancel("Cancelled");
        assert_eq!(record.state, TaskState::Cancelled);
        record.fail("Failed");
        assert_eq!(record.state, TaskState::Failed);
    }

    #[test]
    fn cancellation_is_visible_to_running_jobs() {
        let pool = TaskPool::new(1, 2);
        let started = Arc::new(AtomicBool::new(false));
        let job_started = Arc::clone(&started);
        let handle = pool
            .spawn(move |cancellation| {
                job_started.store(true, Ordering::Release);
                while !cancellation.is_cancelled() {
                    thread::yield_now();
                }
                1
            })
            .unwrap();
        while !started.load(Ordering::Acquire) {
            thread::yield_now();
        }
        handle.cancel();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(completion) = pool.try_completion() {
                assert!(matches!(completion.outcome, TaskOutcome::Completed(1)));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
    }

    #[test]
    fn block_on_parks_until_woken() {
        struct WakeOnce(bool);
        impl Future for WakeOnce {
            type Output = u8;

            fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
                if self.0 {
                    Poll::Ready(7)
                } else {
                    self.0 = true;
                    context.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
        assert_eq!(block_on(WakeOnce(false)), 7);
    }
}
