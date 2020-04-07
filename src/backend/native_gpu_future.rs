use std::{
    task::{Context, Poll, Waker},
    future::Future,
    marker::PhantomPinned,
    pin::Pin,
};
use parking_lot::{Mutex, MutexGuard};
use crate::BufferAddress;

enum WakerOrResult<T> {
    Waker(Waker),
    Result(T),
}

pub(crate) struct Completer<T> {
    waker_or_result: Mutex<Option<WakerOrResult<T>>>,
    buffer_id: wgc::id::BufferId,
    size: BufferAddress,
}

impl<T> Completer<T> {
    fn new(buffer_id: wgc::id::BufferId, size: BufferAddress) -> Self {
        Self {
            waker_or_result: Mutex::new(None),
            buffer_id,
            size,
        }
    }

    pub fn complete(self: Pin<&Self>, value: T) {
        let mut waker_or_result = self.waker_or_result.lock();

        match waker_or_result.replace(WakerOrResult::Result(value)) {
            Some(WakerOrResult::Waker(waker)) => waker.wake(),
            None => {}
            Some(WakerOrResult::Result(_)) => {
                // Drop before panicking. Not sure if this is necessary, but it makes me feel better.
                drop(waker_or_result);
                unreachable!()
            },
        };
    }

    fn lock(&self) -> MutexGuard<Option<WakerOrResult<T>>> {
        self.waker_or_result.lock()
    }

    pub fn get_buffer_info(&self) -> (wgc::id::BufferId, BufferAddress) {
        (self.buffer_id, self.size)
    }
}

/// A Future that can poll the wgpu::Device
pub(crate) struct GpuFuture<T, F> {
    completer: Completer<T>,
    init: Option<F>,
    _unpin: PhantomPinned,
}

impl<T, F> GpuFuture<T, F>
where
    F: FnOnce(Pin<&Completer<T>>)
{
    pub fn create(buffer_id: wgc::id::BufferId, size: BufferAddress, init: F) -> Self {
        Self {
            completer: Completer::new(buffer_id, size),
            init: Some(init),
            _unpin: PhantomPinned,
        }
    }
}

impl<T, F> Future for GpuFuture<T, F>
where
    F: FnOnce(Pin<&Completer<T>>)
{
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context) -> Poll<Self::Output> {
        let (init, completer) = unsafe {
            (
                // Take the init function.
                self.as_mut().get_unchecked_mut().init.take(),
                // Map the Pinned GpuFuture to a pinned Completer.
                // The completer will be in the same location as long
                // as the GpuFuture is pinned, which is either until it completes
                // or it is dropped.
                self.as_ref().map_unchecked(|this| &this.completer),
            )
        };

        if let Some(init) = init {
            init(completer)
        }

        let mut waker_or_result = completer.lock();

        match waker_or_result.take() {
            Some(WakerOrResult::Result(res)) => Poll::Ready(res),
            _ => {
                *waker_or_result = Some(WakerOrResult::Waker(context.waker().clone()));
                Poll::Pending
            }
        }
    }
}

impl<T, F> Drop for GpuFuture<T, F> {
    fn drop(&mut self) {
        // Cancel the buffer mapping if the future is dropped.
        // Without this, the mapping process would be unsound, since the
        // location where the result would be placed would no longer exist.
        let waker_or_result = self.completer.lock();
        
        // If we've already kicked off the mapping process and are waiting
        // for it to map, unmap it to cancel the async mapping process.
        // 
        // If it's already mapped, but we caught it right between getting
        // mapped and being put into the `waker_or_result`, this should
        // still work.
        if let Some(WakerOrResult::Waker(_)) = &*waker_or_result {
            wgn::wgpu_buffer_unmap(self.completer.buffer_id);
        }
    }
}