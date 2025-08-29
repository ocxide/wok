pub trait AsyncRuntime: Send + Sync + 'static {
    type JoinHandle<Out>: JoinHandle<Out>
    where
        Out: Send + 'static;

    fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<<Fut as Future>::Output>
    where
        Fut: Future + Send + 'static,
        Fut::Output: Send + 'static;
}

pub struct FutSpawnError;

pub trait JoinHandle<Out: Send + 'static>:
    Future<Output = Result<Out, FutSpawnError>> + Send
{
}

mod tokio {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::FutureExt;

    use super::{AsyncRuntime, FutSpawnError};

    impl AsyncRuntime for tokio::runtime::Handle {
        type JoinHandle<Out>
            = TokioJoinHandle<Out>
        where
            Out: Send + 'static;

        fn spawn<Fut>(&self, fut: Fut) -> Self::JoinHandle<<Fut as Future>::Output>
        where
            Fut: Future + Send + 'static,
            Fut::Output: Send + 'static,
        {
            TokioJoinHandle(self.spawn(fut))
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

    impl<Out: Send + 'static> super::JoinHandle<Out> for TokioJoinHandle<Out> {}
}
