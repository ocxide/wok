pub trait AsyncExecutorabel {
    type AsyncRuntime: AsyncExecutor;
    fn create() -> Self::AsyncRuntime;
}

pub trait AsyncExecutor: Send + Sync + 'static {
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
