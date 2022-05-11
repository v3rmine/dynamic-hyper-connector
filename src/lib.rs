use futures_util::future::BoxFuture;
use hyper::Uri;
use hyperlocal::UnixConnector;
use tower::Service;
use tracing::info;

type UnixStream = <UnixConnector as Service<Uri>>::Response;

#[derive(Clone)]
struct DynamicConnector {
    inner: UnixConnector,
}

impl Service<Uri> for DynamicConnector {
    type Response = UnixStream;
    type Error = std::io::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Uri) -> Self::Future {
        info!("Must connect to {req}");
        self.inner.call(req)
    }
}

#[cfg(test)]
mod tests {
    use crate::DynamicConnector;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Client, Request, Response, Server,
    };
    use hyperlocal::{UnixConnector, UnixServerExt};
    use test_log::test;

    #[test(tokio::test)]
    async fn test() -> Result<(), color_eyre::Report> {
        let make_service = make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(|_req| async {
                Ok::<_, hyper::Error>(Response::new(Body::from("It works!")))
            }))
        });

        let path = "/tmp/hyperlocal.sock";
        tokio::fs::remove_file(path).await?;

        let server = Server::bind_unix(path)?;
        tokio::spawn(async move {
            server.serve(make_service).await.unwrap();
        });

        let conn = DynamicConnector {
            inner: UnixConnector::default(),
        };
        let client = Client::builder().build(conn);

        let uri = hyperlocal::Uri::new(path, "/hello");
        let req = Request::builder().uri(uri).body(Body::empty())?;
        let res = client.request(req).await?;
        dbg!(res);

        Ok(())
    }
}
