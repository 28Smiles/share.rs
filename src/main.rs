mod config;
mod store;

use std::fs::create_dir;

use crate::config::{Config, UserData};
use crate::store::{Bucket, StorageFile, UserDir};
use actix_multipart::Multipart;
use actix_web::web::Query;
use actix_web::{
    delete, get, post, web, App, Error, HttpRequest, HttpResponse, HttpServer, Result,
};
use futures::TryStreamExt;
use serde::Deserialize;
use std::path::Path;
use urlencoding::encode;

#[derive(Deserialize, Clone)]
struct AuthQuery {
    username: String,
    auth: String,
}

fn is_authed<'a>(data: &'a Config, username: &str, auth: &str) -> Option<&'a UserData> {
    let username = String::from(username);
    match data.users.get(&username) {
        Some(userdata) => {
            if auth == userdata.key {
                Some(userdata)
            } else {
                None
            }
        }
        None => None,
    }
}

fn is_authed_header<'a>(data: &'a Config, request: &HttpRequest) -> Option<&'a UserData> {
    let headers = request.headers();
    let username = headers.get("username").map(|user| user.to_str().unwrap());
    let auth = headers.get("auth").map(|user| user.to_str().unwrap());

    if username.is_some() && auth.is_some() {
        is_authed(data, &username.unwrap(), &auth.unwrap())
    } else {
        None
    }
}

fn is_authed_query<'a>(data: &'a Config, auth_query: &AuthQuery) -> Option<&'a UserData> {
    let username = auth_query.username.as_str();
    let auth = auth_query.auth.as_str();

    is_authed(data, &username, &auth)
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;

    let addr = format!("{}:{}", config.host, config.port);
    println!("Starting Server at {}", addr);
    println!("Registering users:");
    config.users.iter().for_each(|(username, userdata)| {
        println!(
            "username=\"{}\" and folder=\"{}\"",
            username, userdata.folder
        )
    });
    let storage_folder = Path::new(config.storage_folder.as_str());
    create_dir(storage_folder).unwrap_or(());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(config.clone()))
            .service(upload_file)
            .service(get_delete_file)
            .service(delete_file)
            .service(find_file)
    })
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}

#[get("/{user}/{bucket}/{filename}")]
async fn find_file(
    path: web::Path<(String, String, String)>,
    config: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let (user, bucket, filename) = path.into_inner();
    if let Some(userdata) = config.users.get(&*user) {
        let user_dir = UserDir::new(&config, userdata);
        let bucket = Bucket::new(&user_dir, Some(bucket)).unwrap();
        let storage_file = StorageFile::new(&bucket, filename);

        println!(
            "Attempt Serving File from: {}/{}/{}",
            &userdata.folder, &bucket.name, &storage_file.name
        );
        storage_file.serve(&req).await
    } else {
        HttpResponse::NotFound().finish()
    }
}

#[delete("/{bucket}/{filename}")]
async fn delete_file(
    path: web::Path<(String, String)>,
    config: web::Data<Config>,
    request: HttpRequest,
) -> Result<HttpResponse, Error> {
    let (bucket, filename) = path.into_inner();
    if let Some(userdata) = is_authed_header(config.get_ref(), &request) {
        let user_dir = UserDir::new(&config, userdata);
        let bucket = Bucket::new(&user_dir, Some(bucket)).unwrap();
        let storage_file = StorageFile::new(&bucket, filename);

        println!(
            "Deleting File from: {}/{}/{}",
            &userdata.folder, &bucket.name, &storage_file.name
        );
        storage_file.delete().await?;

        Ok(HttpResponse::Ok().body("File Deleted"))
    } else {
        Ok(HttpResponse::Forbidden().finish())
    }
}

#[get("delete/{bucket}/{filename}")]
async fn get_delete_file(
    path: web::Path<(String, String)>,
    config: web::Data<Config>,
    query: Query<AuthQuery>,
) -> Result<HttpResponse, Error> {
    let (bucket, filename) = path.into_inner();
    if let Some(userdata) = is_authed_query(config.get_ref(), &query) {
        let user_dir = UserDir::new(&config, userdata);
        let bucket = Bucket::new(&user_dir, Some(bucket)).unwrap();
        let storage_file = StorageFile::new(&bucket, filename);

        println!(
            "Deleting File from: {}/{}/{}",
            &userdata.folder, &bucket.name, &storage_file.name
        );
        storage_file.delete().await?;

        Ok(HttpResponse::Ok().body("File Deleted"))
    } else {
        Ok(HttpResponse::Forbidden().finish())
    }
}

#[post("/")]
async fn upload_file(
    mut payload: Multipart,
    config: web::Data<Config>,
    request: HttpRequest,
) -> Result<HttpResponse, Error> {
    if let Some(user_data) = is_authed_header(config.get_ref(), &request) {
        let mut files: Vec<String> = Vec::new();
        while let Ok(Some(mut field)) = payload.try_next().await {
            let content_type = field.content_disposition();
            let user_dir = UserDir::new(&config, user_data);
            let bucket = Bucket::new(&user_dir, None).unwrap();
            let storage_file =
                StorageFile::new(&bucket, content_type.get_filename().unwrap().into());

            println!(
                "Uploading File to: {}/{}/{}",
                user_data.folder, &bucket.name, &storage_file.name
            );
            storage_file.write(&mut field).await?;

            files.push(format!(
                "{}/{}/{}",
                encode(&*user_data.folder),
                encode(&*bucket.name),
                encode(&*storage_file.name)
            ));
        }
        Ok(HttpResponse::Ok().body(files.join(",")))
    } else {
        Ok(HttpResponse::Forbidden().finish())
    }
}

#[cfg(test)]
mod tests {
    mod test_find_file {
        use crate::{find_file, Bucket, Config, StorageFile, UserDir};
        use actix_web::http::StatusCode;
        use actix_web::{test, web, App};
        use std::io::Write;

        #[actix_web::test]
        async fn file_404() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(find_file),
            )
            .await;

            let (user, _) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::get()
                .uri(&*format!("/{}/bucket/file.txt", user))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }

        #[actix_web::test]
        async fn user_404() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(find_file),
            )
            .await;

            let req = test::TestRequest::get()
                .uri("/whothat/bucket/file.txt")
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }

        #[actix_web::test]
        async fn file_200() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(find_file),
            )
            .await;

            let filename = "file.txt";
            let (user, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let user_dir = UserDir::new(&config, user_data);
            let bucket = Bucket::new(&user_dir, None).unwrap();
            let storage_file = StorageFile::new(&bucket, filename.into());
            {
                let mut file = storage_file.open(true).await.unwrap();
                file = web::block(move || file.write_all(b"This is a testfile!").map(|_| file))
                    .await
                    .unwrap()
                    .unwrap();
                web::block(move || file.flush()).await.unwrap().unwrap();
            }

            let req = test::TestRequest::get()
                .uri(&*format!(
                    "/{}/{}/{}",
                    user, &bucket.name, &storage_file.name
                ))
                .to_request();
            let resp = test::call_service(&app, req).await;

            storage_file.delete().await.unwrap();

            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    mod test_get_delete_file {
        use crate::{get_delete_file, Bucket, Config, StorageFile, UserDir};
        use actix_web::http::StatusCode;
        use actix_web::{test, web, App};
        use std::io::Write;

        #[actix_web::test]
        async fn file_200() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(get_delete_file),
            )
            .await;

            let filename = "file.txt";
            let (user, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let user_dir = UserDir::new(&config, user_data);
            let bucket = Bucket::new(&user_dir, None).unwrap();
            let storage_file = StorageFile::new(&bucket, filename.into());
            {
                let mut file = storage_file.open(true).await.unwrap();
                file = web::block(move || file.write_all(b"This is a testfile!").map(|_| file))
                    .await
                    .unwrap()
                    .unwrap();
                web::block(move || file.flush()).await.unwrap().unwrap();
            }

            let req = test::TestRequest::get()
                .uri(&*format!(
                    "/delete/{}/{}?username={}&auth={}",
                    &bucket.name, &storage_file.name, user, &user_data.key
                ))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert!(storage_file.open_path(false).await.is_none());
            assert_eq!(resp.status(), StatusCode::OK);
        }

        #[actix_web::test]
        async fn file_404() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(get_delete_file),
            )
            .await;

            let (user, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::get()
                .uri(&*format!(
                    "/delete/bucket/file.txt?username={}&auth={}",
                    user, &user_data.key
                ))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }

        #[actix_web::test]
        async fn file_403_auth() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(get_delete_file),
            )
            .await;

            let (user, _) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::get()
                .uri(&*format!(
                    "/delete/bucket/file.txt?username={}&auth=456",
                    user
                ))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }

        #[actix_web::test]
        async fn file_403_user() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(get_delete_file),
            )
            .await;

            let (_, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::get()
                .uri(&*format!(
                    "/delete/bucket/file.txt?username=someone&auth={}",
                    &user_data.key
                ))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }

        #[actix_web::test]
        async fn file_400_user() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(get_delete_file),
            )
            .await;

            let req = test::TestRequest::get()
                .uri("/delete/bucket/file.txt")
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        }
    }

    mod test_delete_file {
        use crate::{delete_file, Bucket, Config, StorageFile, UserDir};
        use actix_web::http::StatusCode;
        use actix_web::{test, web, App};
        use std::io::Write;

        #[actix_web::test]
        async fn file_200() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(delete_file),
            )
            .await;

            let filename = "file.txt";
            let (user, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let user_dir = UserDir::new(&config, user_data);
            let bucket = Bucket::new(&user_dir, None).unwrap();
            let storage_file = StorageFile::new(&bucket, filename.into());
            {
                let mut file = storage_file.open(true).await.unwrap();
                file = web::block(move || file.write_all(b"This is a testfile!").map(|_| file))
                    .await
                    .unwrap()
                    .unwrap();
                web::block(move || file.flush()).await.unwrap().unwrap();
            }

            let req = test::TestRequest::delete()
                .uri(&*format!("/{}/{}", &bucket.name, &storage_file.name))
                .insert_header(("username", user.clone()))
                .insert_header(("auth", user_data.key.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert!(storage_file.open_path(false).await.is_none());
            assert_eq!(resp.status(), StatusCode::OK);
        }

        #[actix_web::test]
        async fn file_404() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(delete_file),
            )
            .await;

            let (user, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::delete()
                .uri("/bucket/file.txt")
                .insert_header(("username", user.clone()))
                .insert_header(("auth", user_data.key.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        }

        #[actix_web::test]
        async fn file_403() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(delete_file),
            )
            .await;

            let req = test::TestRequest::delete()
                .uri("/bucket/file.txt")
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }

        #[actix_web::test]
        async fn file_403_user() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(delete_file),
            )
            .await;

            let (user, _) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::delete()
                .uri("/bucket/file.txt")
                .insert_header(("username", user.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }

        #[actix_web::test]
        async fn file_403_auth() {
            let config = Config::default();
            let app = test::init_service(
                App::new()
                    .app_data(web::Data::new(config.clone()))
                    .service(delete_file),
            )
            .await;

            let (_, user_data) = *(&config.users).into_iter().peekable().peek().unwrap();
            let req = test::TestRequest::delete()
                .uri("/bucket/file.txt")
                .insert_header(("auth", user_data.key.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;

            assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        }
    }
}
