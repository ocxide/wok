#[cfg(feature = "tokio")]
pub mod tokio {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::FutureExt;

    use wok_core::async_executor::{AsyncExecutor, AsyncExecutorabel, FutSpawnError, JoinHandle};

    pub struct TokioRt;

    impl AsyncExecutorabel for TokioRt {
        type AsyncRuntime = TokioRuntimeHandle;
        fn create() -> Self::AsyncRuntime {
            TokioRuntimeHandle(tokio::runtime::Handle::current())
        }
    }

    pub struct TokioRuntimeHandle(tokio::runtime::Handle);

    impl AsyncExecutor for TokioRuntimeHandle {
        type JoinHandle<Out>
            = TokioJoinHandle<Out>
        where
            Out: Send + 'static;

        fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<<Fut as Future>::Output>
        where
            Fut: Future + Send + 'static,
            Fut::Output: Send + 'static,
        {
            TokioJoinHandle(self.0.spawn(fut))
        }
    }

    pub struct TokioJoinHandle<Out: Send + 'static>(tokio::task::JoinHandle<Out>);
    impl<Out: Send + 'static> Future for TokioJoinHandle<Out> {
        type Output = Result<Out, FutSpawnError>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            match self.0.poll_unpin(cx) {
                Poll::Ready(res) => Poll::Ready(res.map_err(|_| FutSpawnError)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl<Out: Send + 'static> JoinHandle<Out> for TokioJoinHandle<Out> {}
}
