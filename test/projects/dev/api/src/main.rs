use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::io;

use actix_web::{
    client::Client,
    error::ErrorBadRequest,
    web::{self, BytesMut},
    App, Error, HttpResponse, HttpServer, HttpRequest
};
use futures::{future,StreamExt};
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

async fn ok(
    req: HttpRequest
) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().content_type("text/plain").body("OK"))
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    let target = std::env::var("DEPLO_RELEASE_TARGET").unwrap_or("dev".to_string());
    
    println!("Starting {} server at port 80(api),10000(admin),10001(metrics)", target);
    let s1 = HttpServer::new(move || {
        let version = std::env::var("DEPLO_SERVICE_VERSION").unwrap();
        let path = format!("/api/{}", version);
        App::new()
            .data(Client::default())
            .service(web::resource(format!("{}/something", &path)).route(web::post().to(create_something)))
            .service(web::resource(format!("{}/ping", &path)).route(web::get().to(ok)))
            .service(web::resource("/ping".to_string()).route(web::get().to(ok)))
    })
    .bind("0.0.0.0:80")?
    .run();

    let s2 = HttpServer::new(move || {
        let version = std::env::var("DEPLO_SERVICE_VERSION").unwrap();
        let path = format!("/admin/{}", version);
        App::new()
            .data(Client::default())
            .service(web::resource(format!("{}/ping", &path)).route(web::get().to(ok)))
            .service(web::resource("/ping".to_string()).route(web::get().to(ok)))
    })
    .bind("0.0.0.0:10000")?
    .run();

    let s3 = HttpServer::new(move || {
        let version = std::env::var("DEPLO_SERVICE_VERSION").unwrap();
        let path = format!("/metrics/{}", version);
        App::new()
            .data(Client::default())
            .service(web::resource(format!("{}/ping", &path)).route(web::get().to(ok)))
            .service(web::resource("/ping".to_string()).route(web::get().to(ok)))
    })
    .bind("0.0.0.0:10001")?
    .run();

    future::try_join(future::try_join(s1, s2), s3).await?;
    Ok(())
}
