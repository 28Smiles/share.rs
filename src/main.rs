use std::collections::HashMap;
use std::fs::{File, create_dir};
use std::io::Write;
use std::time::SystemTime;

use actix_multipart::Multipart;
use actix_web::{App, delete, Error, get, HttpServer, post, Result, web, HttpResponse};
use actix_web::web::Query;
use futures::{StreamExt, TryStreamExt};
use rand::Rng;
use serde::Deserialize;
use std::path::{Path, PathBuf};


struct Config {
    host: String,
    port: i64,
    storage_folder: String,
    users: HashMap<String, UserData>,
}

impl Clone for Config {
    fn clone(&self) -> Self {
        Config {
            host: self.host.clone(),
            port: self.port,
            storage_folder: self.storage_folder.clone(),
            users: self.users.iter().map(|(username, data)| (username.clone(), data.clone())).collect(),
        }
    }
}

trait Alphanumeric {
    fn is_alphanumeric(&self) -> bool;
}

impl Alphanumeric for String {
    fn is_alphanumeric(&self) -> bool {
        self.chars().all(|c| c.is_alphanumeric())
    }
}

#[derive(Deserialize, Clone)]
struct AuthQuery {
    username: Option<String>,
    auth: String,
}

#[derive(Clone)]
struct UserData {
    key: String,
    folder: String,
}

async fn load_config() -> Config {
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name("config")).unwrap();

    let host = settings.get_str("host").unwrap();
    let port = settings.get_int("port").unwrap();
    let storage_folder = settings.get_str("storage-folder").unwrap();
    let users = settings.get_table("users").unwrap();

    Config {
        host,
        port,
        storage_folder,
        users: users.into_iter()
            .map(|(username, data)| (username, data.into_table().unwrap()))
            .map(move |(username, data)| (username, UserData {
                key: data.get("key").unwrap().to_owned().into_str().unwrap(),
                folder: data.get("folder").unwrap().to_owned().into_str().unwrap(),
            }))
            .collect(),
    }
}

fn is_authed<'a>(
    data: &'a Config,
    username: &Option<&str>,
    auth: &Option<&str>,
) -> Option<&'a UserData> {
    match *username {
        Some(username) => {
            let username = String::from(username);
            match data.users.get(&username) {
                Some(userdata) => match auth {
                    Some(provided_auth_key) => if *provided_auth_key == userdata.key { Some(userdata) } else { None },
                    None => None
                },
                None => None
            }
        }
        None => None
    }
}

fn is_authed_header<'a>(
    data: &'a Config,
    request: &web::HttpRequest,
) -> Option<&'a UserData> {
    let headers = request.headers();
    let username = headers.get("user").map(|user| user.to_str().unwrap());
    let auth = headers.get("auth").map(|user| user.to_str().unwrap());

    is_authed(data, &username, &auth)
}

fn is_authed_query<'a>(
    data: &'a Config,
    auth_query: &AuthQuery,
) -> Option<&'a UserData> {
    let username = auth_query.username.as_ref().map(|s| s.as_str());
    let auth = Some(auth_query.auth.as_str());

    is_authed(
        data,
        &username,
        &auth,
    )
}

fn remove_file(
    userdata: &UserData,
    bucket: String,
    filename: String,
    config: &Config,
) -> web::HttpResponse {
    let path = sub_folder(config, userdata)
        .join(&bucket)
        .join(sanitize_filename::sanitize(filename));
    if path.exists() {
        std::fs::remove_file(path.to_str().unwrap())
            .map(|()| std::fs::remove_dir(sub_folder(config, userdata).join(bucket).to_str().unwrap()).unwrap_or(()))
            .map(|()| std::fs::remove_dir(sub_folder(config, userdata).to_str().unwrap()).unwrap_or(()))
            .map(|()| web::HttpResponse::Ok().body("File Deleted"))
            .unwrap_or(web::HttpResponse::NotFound().finish())
    } else {
        web::HttpResponse::NotFound().finish()
    }
}

fn sub_folder(
    config: &Config,
    data: &UserData,
) -> PathBuf {
    Path::new(config.storage_folder.as_str()).join(&data.folder)
}

fn sha256(value: &String) -> String {
    base32::encode(
        base32::Alphabet::RFC4648 { padding: false },
        ring::digest::digest(&ring::digest::SHA256, value.as_bytes()).as_ref(),
    ).to_lowercase()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = load_config().await;

    let addr = format!("{}:{}", config.host, config.port);
    println!("Starting Server at {}", addr);
    println!("Registering users:");
    config.users.iter().for_each(|(username, userdata)|
        println!("username=\"{}\" and folder=\"{}\"",
                 username,
                 userdata.folder)
    );
    let storage_folder = Path::new(config.storage_folder.as_str());
    create_dir(storage_folder).unwrap_or(());

    HttpServer::new(move || {
        App::new()
            .data(config.to_owned())
            .service(upload_file)
            .service(get_delete_file)
            .service(delete_file)
            .service(find_file)
    }).bind(addr)?.run().await
}

#[get("/{user}/{bucket}/{filename}")]
async fn find_file(
    web::Path((user, bucket, filename)): web::Path<(String, String, String)>,
    config: web::Data<Config>,
    req: web::HttpRequest,
) -> web::HttpResponse {
    let userdata = config.users.get(&*user);
    if userdata.is_some() && bucket.chars().all(|c| c.is_alphanumeric()) {
        println!("Serving File from: {}/{}/{}", userdata.unwrap().folder, bucket, filename);
        actix_files::NamedFile::open(sub_folder(config.get_ref(), userdata.unwrap())
            .join(bucket)
            .join(sanitize_filename::sanitize(filename)))
            .map(|file| file.into_response(&req))
            .unwrap_or(Ok(web::HttpResponse::NotFound().finish()))
            .unwrap_or(web::HttpResponse::NotFound().finish())
    } else {
        HttpResponse::BadRequest().finish()
    }
}

#[delete("/{bucket}/{filename}")]
async fn delete_file(
    web::Path((bucket, filename)): web::Path<(String, String)>,
    config: web::Data<Config>,
    request: web::HttpRequest,
) -> web::HttpResponse {
    let userdata = is_authed_header(config.get_ref(), &request);
    if userdata.is_some() && bucket.is_alphanumeric() {
        println!("Deleting File from: {}/{}/{}", userdata.unwrap().folder, bucket, filename);
        remove_file(userdata.unwrap(), bucket, filename, &config)
    } else {
        web::HttpResponse::Forbidden().finish()
    }
}

#[get("delete/{bucket}/{filename}")]
async fn get_delete_file(
    web::Path((bucket, filename)): web::Path<(String, String)>,
    config: web::Data<Config>,
    query: Query<AuthQuery>,
) -> web::HttpResponse {
    let userdata = is_authed_query(config.get_ref(), &query);
    if userdata.is_some() && bucket.is_alphanumeric() {
        println!("Deleting File from: {}/{}/{}", userdata.unwrap().folder, bucket, filename);
        remove_file(userdata.unwrap(), bucket, filename, &config)
    } else {
        web::HttpResponse::Forbidden().finish()
    }
}

#[post("/")]
async fn upload_file(
    mut payload: Multipart,
    config: web::Data<Config>,
    request: web::HttpRequest,
) -> Result<web::HttpResponse, Error> {
    match is_authed_header(config.get_ref(), &request) {
        Some(userdata) => {
            let mut files: Vec<String> = Vec::new();
            while let Ok(Some(mut field)) = payload.try_next().await {
                let content_type = field.content_disposition().unwrap();
                let filename = sanitize_filename::sanitize(content_type.get_filename().unwrap());
                let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros();
                let random: String = rand::thread_rng().sample_iter(rand::distributions::Alphanumeric).take(128).collect();
                let bucket: String = sha256(&format!("{}{}", time, random));
                println!("Uploading File to: {}/{}/{}", userdata.folder, bucket, filename);
                let filepath = sub_folder(config.get_ref(), userdata);
                create_dir(&filepath).unwrap_or(());
                let filepath = filepath.join(&bucket);
                create_dir(&filepath).unwrap_or(());
                let filepath = filepath.join(&filename);
                let mut f = web::block(move || File::create(&*filepath)).await.unwrap();
                while let Some(chunk) = field.next().await {
                    let data = chunk.unwrap();
                    f = web::block(move || f.write_all(&data).map(|_| f)).await?;
                }
                files.push(format!("{}/{}/{}", userdata.folder, bucket, filename));
            }
            Ok(web::HttpResponse::Ok().body(files.join(",")))
        }
        None => Ok(web::HttpResponse::Forbidden().finish())
    }
}
