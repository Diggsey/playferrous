use std::any::type_name;

use async_trait::async_trait;

#[async_trait]
pub trait Actor: Sized + Send + 'static {
    async fn run(self) -> anyhow::Result<()>;
    fn spawn(self) {
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                tracing::error!("{}: {}", type_name::<Self>(), e);
            }
        });
    }
}

#[macro_export]
macro_rules! select_recv_loop {
    ($($v:pat = $a:expr => $b:expr,)*) => {
        loop {
            tokio::select! {
                biased;
                $(
                    _msg = $a => if let Some(_msg) = _msg {
                        let $v = _msg;
                        $b
                    } else {
                        break;
                    },
                )*
            }
        }
    };
}
