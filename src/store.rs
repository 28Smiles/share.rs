use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use actix_files::NamedFile;
use actix_multipart::Field;
use actix_web::error::{ErrorInternalServerError, ErrorNotFound};
use actix_web::{HttpRequest, HttpResponse, web};
use futures::{StreamExt};
use rand::Rng;
use crate::{Config, UserData};

pub struct UserDir<'a, 'b> {
    config: &'a Config,
    user_data: &'b UserData
}

impl <'a, 'b>UserDir<'a, 'b> {
    pub fn new(config: &'a Config, user_data: &'b UserData) -> Self {
        UserDir {
            config,
            user_data
        }
    }

    pub fn path(&self) -> PathBuf {
        Path::new(&self.config.storage_folder).join(&self.user_data.folder)
    }

    pub async fn open(&self, create: bool) -> Option<PathBuf> {
        let path = self.path();
        if create {
            let path = path.clone();
            web::block(move || fs::create_dir(&path)).await
                .unwrap_or(Ok(()))
                .unwrap_or(());
        }

        if {
            let path = path.clone();
            web::block(move || path.exists()).await.unwrap()
        } {
            Some(path)
        } else {
            None
        }
    }

    pub async fn try_delete(&self) -> Result<(), actix_web::error::Error> {
        if let Some(path) = self.open(false).await {
            let files = {
                let path = path.clone();
                web::block(move || fs::read_dir(&path)).await.unwrap()?.count()
            };
            if files == 0 {
                web::block(move || fs::remove_dir(&path)).await
                    .unwrap()
                    .map_err(|_| ErrorInternalServerError("Can't Delete User Dir"))
            } else {
                Ok(())
            }
        } else {
            Err(ErrorNotFound("Can't Find User Dir"))
        }
    }
}

pub struct Bucket<'a, 'b, 'c> {
    user_dir: &'a UserDir<'b, 'c>,
    pub(crate) name: String
}

impl <'a, 'b, 'c>Bucket<'a, 'b, 'c> {
    pub fn new(user_dir: &'a UserDir<'b, 'c>, name: Option<String>) -> Option<Self> {
        if let Some(name) = name {
            if name.chars().all(|char| char.is_alphanumeric()) {
                Some(Bucket {
                    user_dir,
                    name
                })
            } else {
                None
            }
        } else {
            Some(Bucket {
                user_dir,
                name: String::from_utf8(rand::thread_rng()
                    .sample_iter(rand::distributions::Alphanumeric)
                    .take(16)
                    .collect()).unwrap()
            })
        }
    }

    pub async fn open(&self, create: bool) -> Option<PathBuf> {
        if let Some(path) = self.user_dir.open(create).await {
            let path = path.join(&self.name);
            if create {
                let path = path.clone();
                web::block(move || fs::create_dir(&path)).await
                    .unwrap_or(Ok(()))
                    .unwrap_or(());
            }

            if {
                let path = path.clone();
                web::block(move || path.exists()).await.unwrap()
            } {
                Some(path)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub async fn try_delete(&self) -> Result<(), actix_web::error::Error> {
        if let Some(path) = self.open(false).await {
            let files = {
                let path = path.clone();
                web::block(move || fs::read_dir(&path)).await.unwrap()?.count()
            };
            if files == 0 {
                web::block(move || fs::remove_dir(&path)).await
                    .unwrap()
                    .map_err(|_| ErrorInternalServerError("Can't Delete Bucket"))?;

                self.user_dir.try_delete().await
            } else {
                Ok(())
            }
        } else {
            Err(ErrorNotFound("Can't Find Bucket"))
        }
    }
}

pub struct StorageFile<'a, 'b, 'c, 'd> {
    bucket: &'a Bucket<'b, 'c, 'd>,
    pub name: String
}

impl <'a, 'b, 'c, 'd>StorageFile<'a, 'b, 'c, 'd> {
    pub fn new(bucket: &'a Bucket<'b, 'c, 'd>, name: String) -> Self {
        StorageFile {
            bucket,
            name: sanitize_filename::sanitize(name)
        }
    }

    pub async fn open(&self, create: bool) -> Option<File> {
        if let Some(path) = self.bucket.open(create).await {
            let path = path.join(&self.name);

            if {
                let path = path.clone();
                web::block(move || path.exists()).await.unwrap()
            } {
                if create {
                    None
                } else {
                    web::block(move || File::open(&path)).await.unwrap()
                        .map(|file| Some(file))
                        .unwrap_or(None)
                }
            } else {
                if create {
                    web::block(move || File::create(&path)).await.unwrap()
                        .map(|file| Some(file))
                        .unwrap_or(None)
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub async fn open_path(&self, create: bool) -> Option<PathBuf> {
        if let Some(path) = self.bucket.open(create).await {
            let path = path.join(&self.name);

            if {
                let path = path.clone();
                web::block(move || path.exists()).await.unwrap()
            } {
                Some(path)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub async fn write(&self, field: &mut Field) -> Result<(), actix_web::error::Error> {
        if let Some(mut file) = self.open(true).await {
            while let Some(chunk) = field.next().await {
                let data = chunk.unwrap();
                file = web::block(move || file.write_all(&data).map(|_| file)).await??;
            }

            Ok(())
        } else {
            Err(ErrorInternalServerError("Cant write to file"))
        }
    }

    pub async fn delete(&self) -> Result<(), actix_web::error::Error> {
        if let Some(path) = self.open_path(false).await {
            web::block(move || fs::remove_file(&path)).await
                .unwrap()
                .map_err(|_| ErrorInternalServerError("File Can not ne deleted"))?;

            self.bucket.try_delete().await
        } else {
            Err(ErrorNotFound("File Not Found"))
        }
    }

    pub async fn serve(&self, req: &HttpRequest) -> HttpResponse {
        if let Some(path) = self.open_path(false).await {
            if let Ok(response) = NamedFile::open(path).map(|file| file.into_response(req)) {
                response
            } else {
                HttpResponse::NotFound().finish()
            }
        } else {
            HttpResponse::NotFound().finish()
        }
    }
}
