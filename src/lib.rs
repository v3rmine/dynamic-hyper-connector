use std::{
    io::Error,
    pin::Pin,
    task::{Context, Poll},
};

use delegate::delegate;
use futures_util::future::BoxFuture;
use hyper::{
    client::{connect::Connection, HttpConnector},
    Uri,
};
use hyperlocal::UnixConnector;
use pin_project_lite::pin_project;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
};
use tokio_util::either::Either;
use tower::Service;
use tracing::debug;

// hyperlocal has its own wrapper over `tokio::net::UnixStream` and they don't
// export it, so... that's one way to be able to name that type.
type UnixStream = <UnixConnector as Service<Uri>>::Response;

pin_project! {
    // `Either` takes care of unifying the two underlying streams
    // but only implements `AsyncRead`/`AsyncWrite`: we want it to
    // implement `Connection` too.
    struct DynamicConnection {
        #[pin]
        inner: Either<UnixStream, TcpStream>,
    }
}

impl From<Either<UnixStream, TcpStream>> for DynamicConnection {
    fn from(inner: Either<UnixStream, TcpStream>) -> Self {
        Self { inner }
    }
}

// Just delegating I/O stuff to `Either`...
impl AsyncRead for DynamicConnection {
    delegate! {
        to self.project().inner {
            fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>>;
        }
    }
}

// Just delegating I/O stuff to `Either`...
impl AsyncWrite for DynamicConnection {
    delegate! {
        to self.project().inner {
            fn poll_write(
                self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buf: &[u8],
            ) -> Poll<Result<usize, std::io::Error>>;
            fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>>;
            fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>>;
        }
    }
}

// Both variants of the `Either` implement Connection, we can just ask them.
impl Connection for DynamicConnection {
    fn connected(&self) -> hyper::client::connect::Connected {
        match &self.inner {
            Either::Left(stream) => stream.connected(),
            Either::Right(stream) => stream.connected(),
        }
    }
}

#[derive(Clone)]
struct DynamicConnector {
    unix: UnixConnector,
    http: HttpConnector,
}

impl Default for DynamicConnector {
    fn default() -> Self {
        Self {
            unix: Default::default(),
            http: HttpConnector::new(),
        }
    }
}

impl Service<Uri> for DynamicConnector {
    type Response = DynamicConnection;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if let Err(e) = futures::ready!(self.unix.poll_ready(cx)) {
            return Err(e).into();
        }
        if let Err(e) = futures::ready!(self.http.poll_ready(cx)) {
            return Err(Error::new(std::io::ErrorKind::Other, e)).into();
        }
        Ok(()).into()
    }

    fn call(&mut self, req: Uri) -> Self::Future {
        debug!("Must connect to {req}");
        if let Some("unix") = req.scheme_str() {
            debug!("Using unix connector");
            let fut = self.unix.call(req);
            Box::pin(async move {
                let res = fut.await?;
                Ok(Either::Left(res).into())
            })
        } else {
            debug!("Using http connector");
            let fut = self.http.call(req);
            Box::pin(async move {
                let res = fut
                    .await
                    .map_err(|e| Error::new(std::io::ErrorKind::Other, e))?;
                Ok(Either::Right(res).into())
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::DynamicConnector;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Client, Request, Response, Server,
    };
    use hyperlocal::UnixServerExt;
    use test_log::test;
    use tracing::info;

    #[test(tokio::test)]
    async fn test() -> Result<(), color_eyre::Report> {
        let path = "/tmp/hyperlocal.sock";
        tokio::fs::remove_file(path).await?;

        let unix_make_service = make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(|_req| async {
                Ok::<_, hyper::Error>(Response::new(Body::from("It works!")))
            }))
        });

        let unix_server = Server::bind_unix(path)?;
        tokio::spawn(async move {
            unix_server.serve(unix_make_service).await.unwrap();
        });

        let http_make_service = make_service_fn(|_| async {
            Ok::<_, hyper::Error>(service_fn(|_req| async {
                Ok::<_, hyper::Error>(Response::new(Body::from("It works!")))
            }))
        });

        let http_addr = "127.0.0.1:5772";
        let http_server = Server::bind(&http_addr.parse()?);
        tokio::spawn(async move {
            http_server.serve(http_make_service).await.unwrap();
        });

        let conn = DynamicConnector::default();
        let client = Client::builder().build(conn);

        let req = Request::builder()
            .uri(hyperlocal::Uri::new(path, "/hello"))
            .body(Body::empty())?;
        let res = client.request(req).await?;
        info!(?res, "Did unix request");

        let req = Request::builder()
            .uri(format!("http://{http_addr}/hello"))
            .body(Body::empty())?;
        let res = client.request(req).await?;
        info!(?res, "Did http request");

        Ok(())
    }
}
