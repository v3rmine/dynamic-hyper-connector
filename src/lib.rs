use futures_util::future::BoxFuture;
use hyper::{client::connect::Connect, Uri};
use hyperlocal::UnixConnector;
use tower::Service;

type UnixStream = <UnixConnector as Service<Uri>>::Response;

pub fn new_connector(
    url: &str,
) -> impl Connect
       + Clone
       + Service<
    Uri,
    Response = UnixStream,
    Error = std::io::Error,
    Future = BoxFuture<'static, Result<UnixStream, std::io::Error>>,
> {
    // if url.starts_with("unix://") {
    //     Either::A(UnixConnector::default())
    // } else {
    //     Either::B(HttpConnector::new())
    // }
    UnixConnector::default()
}

#[cfg(test)]
mod tests {
    use hyper::{Body, Client, Request};

    use crate::new_connector;

    #[tokio::test]
    async fn test() -> Result<(), Box<dyn std::error::Error>> {
        let url = "unix:///tmp/socket";
        let client = Client::builder().build(new_connector(url));

        let uri = hyperlocal::Uri::new("/tmp/socket", "/hello");
        let req = Request::builder().uri(uri).body(Body::empty())?;
        let res = client.request(req).await?;
        dbg!(res);

        Ok(())
    }
}
