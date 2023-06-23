use crate::*;
use tedge_actors::ClientMessageBox;

#[tokio::test]
async fn get_over_https() {
    let mut request_builder = http::Request::builder();

    // Set request properties
    request_builder = request_builder.method("GET").uri("https://example.com");

    // Clone the request builder
    let cloned_request_builder = request_builder.clone();

    let mut server = mockito::Server::new();
    let _mock = server.mock("GET", "/").create();

    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get(server.url())
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
}

async fn spawn_http_actor() -> ClientMessageBox<HttpRequest, HttpResult> {
    let mut builder = HttpActor::new().builder();
    let handle = ClientMessageBox::new("Tester", &mut builder);

    tokio::spawn(builder.run());

    handle
}
