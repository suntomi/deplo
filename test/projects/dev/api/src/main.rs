use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::io;

use actix_web::{
    client::Client,
    error::ErrorBadRequest,
    web::{self, BytesMut},
    App, Error, HttpResponse, HttpServer,
};
use futures::StreamExt;
use validator::Validate;
use validator_derive::Validate;

#[derive(Debug, Validate, Deserialize, Serialize)]
struct SomeData {
    #[validate(length(min = 1, max = 1000000))]
    id: String,
    #[validate(length(min = 1, max = 100))]
    name: String,
}

#[derive(Debug, Deserialize)]
struct HttpBinResponse {
    args: HashMap<String, String>,
    data: String,
    files: HashMap<String, String>,
    form: HashMap<String, String>,
    headers: HashMap<String, String>,
    json: SomeData,
    origin: String,
    url: String,
}

/// validate data, post json to httpbin, get it back in the response body, return deserialized
async fn step_x(data: SomeData, client: &Client) -> Result<SomeData, Error> {
    // validate data
    data.validate().map_err(ErrorBadRequest)?;

    let mut res = client
        .post("https://httpbin.org/post")
        .send_json(&data)
        .await
        .map_err(Error::from)?; // <- convert SendRequestError to an Error

    let mut body = BytesMut::new();
    while let Some(chunk) = res.next().await {
        body.extend_from_slice(&chunk?);
    }

    let body: HttpBinResponse = serde_json::from_slice(&body).unwrap();
    Ok(body.json)
}

async fn create_something(
    some_data: web::Json<SomeData>,
    client: web::Data<Client>,
) -> Result<HttpResponse, Error> {
    let some_data_2 = step_x(some_data.into_inner(), &client).await?;
    let some_data_3 = step_x(some_data_2, &client).await?;
    let d = step_x(some_data_3, &client).await?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&d).unwrap()))
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

    let _ = std::env::var("DEPLO_RELEASE_TARGET").unwrap_or("dev".to_string());
    let version = std::env::var("DEPLO_SERVICE_VERSION").unwrap_or("1".to_string());
    let name = std::env::var("DEPLO_SERVICE_NAME").unwrap_or("api".to_string());
    let path = std::env::var("DEPLO_SERVICE_PATH").unwrap_or(format!("/{}/{}", name, version));
    let endpoint = "127.0.0.1:80";

    println!("Starting server at: {:?}", endpoint);
    HttpServer::new(move || {
        App::new()
            .data(Client::default())
            .service(web::resource(format!("{}/something", path)).route(web::post().to(create_something)))
    })
    .bind(endpoint)?
    .run()
    .await
}