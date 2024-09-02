use crate::{FileMetadata, SyncManifest};
use crate::drivers::Driver;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::types::{Delete, ObjectCannedAcl, ObjectIdentifier};
use std::sync::Arc;


// use aws_config::meta::region::RegionProviderChain;
// use aws_config::BehaviorVersion;
use aws_sdk_s3::operation::{
//     copy_object::{CopyObjectError, CopyObjectOutput},
//     create_bucket::{CreateBucketError, CreateBucketOutput},
    // get_object::{GetObjectError, GetObjectOutput},
//     list_objects_v2::ListObjectsV2Output,
    put_object::{PutObjectError, PutObjectOutput},
    delete_objects::DeleteObjectsOutput,
    
};
use aws_sdk_s3::{Client, Error, error::SdkError};

use std::io::Read;
use aws_sdk_s3::primitives::ByteStream;
use bytes::buf::Buf;
use std::collections::HashMap;


pub struct S3Driver {
    bucket: String,
    common_prefix: String,
    client: Arc<Client>
}

impl S3Driver {
    pub async fn new(bucket: String, common_prefix: String) -> Self {
        //TODO: region?
        let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
        let config = aws_config::from_env()
            .region(region_provider)
            .load().await;
        let client = Client::new(&config);

        S3Driver {
            bucket,
            common_prefix,
            client: Arc::new(client)
        }
    }

    
    pub async fn get_object_reader(&self, key: &str) -> Result<impl std::io::Read, Box<dyn std::error::Error>> {
        let client = &self.client;
        let object = client
            .get_object()
            .bucket(self.bucket.to_owned())
            .key(key)
            .send()
            .await;

        let res = match object {
            Ok(object) => object.body.collect().await.map(|data| data.into_bytes()),
            Err(err) => return Err(Box::new(err))
        };

        let reader = res.expect("If the object exist, it should be possible to get reader").reader();
        
        return Ok(reader);

        // Ok("sd")
        // let rdr = b.into_buf();
        // return data;
        // return std::str::from_utf8(b)
    }

    // pub async fn get_object_writer(&self, key: &str) -> Result<impl std::io::Write, Box<dyn std::error::Error>> {
    //     let client = &self.client;
    //     let object = client
    //         .put_object()
    //         .bucket(self.bucket.to_owned())
    //         .key(key)
    //         .send()
    //         .await;

    //     // let writer: std::io::Write;

    //     return Ok(writer);
    // }

    // pub async fn put_object_file(&self, key: &str, path: String, acl: Option<ObjectCannedAcl>) {
    //     let client = &self.client;
    //     let object = self.put_object_data(key, data, acl)
    //         .send()
    //         .await;
    // }

    pub async fn put_object_data(&self, key: String, metadata: Option<HashMap<String, String>>, data: ByteStream, acl: &str) -> Result<PutObjectOutput, SdkError<PutObjectError>> {
        let objectcannedacl = match acl {
            "authenticated-read" => ObjectCannedAcl::AuthenticatedRead,
            "aws-exec-read" => ObjectCannedAcl::AwsExecRead,
            "bucket-owner-full-control" => ObjectCannedAcl::BucketOwnerFullControl,
            "bucket-owner-read" => ObjectCannedAcl::BucketOwnerRead,
            "private" => ObjectCannedAcl::Private,
            "public" => ObjectCannedAcl::PublicRead,
            "public-read" => ObjectCannedAcl::PublicRead,
            "public-read-write" => ObjectCannedAcl::PublicReadWrite,
            _ => ObjectCannedAcl::Private
            /* other @ _ if other.as_str() == "foo" => { handle foo } */
            //TODO: there should be better way to map this earlier on, to match command line input properly
        };
        
        //TODO: this is a mess!!
        //TODO: should also strip Content-Type from metadata?
        let ctype = match &metadata {
            Some(meta) => match meta.get("Content-Type") {
                Some(ct) => Some(ct.to_string()),
                _ => None
            },
            _ => None
        };

        let client = &self.client;
        return client
            .put_object()
            .bucket(self.bucket.to_owned())
            .key(key)
            .body(data)
            .acl(objectcannedacl)

            .set_metadata(metadata)
            .set_content_type(ctype)

            .send().await;

    }

    pub async fn delete_objects(&self, keys: &Vec<String>) -> Result<(), Error> {
        let client = &self.client;

        if !keys.is_empty() {
            let mut delete_objects: Vec<ObjectIdentifier> = vec![];
            for obj in keys.iter() {
                let obj_id = ObjectIdentifier::builder()
                    .set_key(Some(obj.to_string()))
                    .build()
                    .map_err(Error::from)?;
                delete_objects.push(obj_id);
            }

            _ = client
                .delete_objects()
                .bucket(&self.bucket)
                .delete(
                    Delete::builder()
                        .set_objects(Some(delete_objects))
                        .build()
                        .map_err(Error::from)?,
                )
                .send()
                .await;
        }

        return Ok(())
    
    }
}

impl Driver for S3Driver {
    async fn build_manifest(&self) -> SyncManifest {
                //TODO skip ignore files

        let mut manifest = SyncManifest::new();
        let client = &self.client;
        let mut response = client
            .list_objects_v2()
            .bucket(self.bucket.to_owned())
            .prefix(&self.common_prefix)
            .into_paginator()
            .send();

        while let Some(result) = response.next().await {
            match result {
                Ok(output) => {
                    for object in output.contents() {
                        let mut path = std::path::PathBuf::from(self.common_prefix.clone());
                        let key = object.key().expect("Each object should have a key.");
                        
                        path.push(key);

                        manifest.files.insert(
                            key.to_string(),
                            FileMetadata {
                                path,
                                size: object.size().expect("Each object should have size") as u64,
                                last_modified: object.last_modified().expect("Each object should have last_modified date").as_secs_f64(),
                                etag: Some(object.e_tag().expect("Each object should have e-tag").to_string()),
                                checksum: None,
                                content_type: None
                            }
                        );
                    }
                }
                Err(err) => {
                    eprintln!("{err:?}")
                }
            }
        }

        return manifest;
    }
}