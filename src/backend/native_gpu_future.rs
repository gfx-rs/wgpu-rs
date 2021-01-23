use parking_lot::Mutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::thread;

#[derive(Clone)]
pub(crate) struct SharedState<T: Clone+ Send> {
    pub completed: bool,
    pub waker: Option<Waker>,
    pub data: Option<T>,
}

pub(crate) fn start_worker_thread(context: Arc<crate::backend::direct::Context>) {
    const POLL_TIME_MS: u64 = 100;
    let wait_duration = std::time::Duration::from_millis(POLL_TIME_MS);
    thread::spawn(move || {
        loop {
            context.0.poll_all_devices(false).expect("Unable to poll");
            thread::sleep(wait_duration);
        }
    });
}

/// A Future that can poll the wgpu::Device
pub struct GpuFuture<T: Clone+ Send> {
    pub(crate) shared_state: Arc<Mutex<SharedState<T>>>,
}

impl<T: Clone + Send> GpuFuture<T> {
    pub(crate) fn new() -> Self {
        GpuFuture {
            shared_state: Arc::new(Mutex::new(SharedState {
                completed: false,
                waker: None,
                data: None,
            })),
        }
    }
}

impl<T: 'static + Clone + Send> Future for GpuFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let mut locked = self.shared_state.lock();
        if let Some(waker) = &mut locked.waker {
            if !waker.will_wake(context.waker()) {
                *waker = context.waker().clone();
            }
        } else {
            locked.waker = Some(context.waker().clone());
        }

        if locked.completed {
            Poll::Ready(locked.data.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
