// use std::error::Error;
use std::fs;
// use std::str::FromStr;
use aws_sdk_s3::primitives::ByteStream;
use clap::Parser;

// use serde_json::Result;
// use strum_macros::EnumString;

use deploy_aws_s3::drivers::fsdriver::FSDriver;
use deploy_aws_s3::drivers::s3driver::S3Driver;
use deploy_aws_s3::drivers::Driver;
use deploy_aws_s3::SyncManifest;

use bytes::Bytes;

use mime_guess::mime;
use std::collections::HashMap;

#[derive(Parser, Debug)]
struct CLIArgs {
    #[arg(short = 'b', long = "bucket")]
    bucket: String,

    #[arg(short = 'p', long = "common-prefix", required = false, default_value = "")]
    prefix: String,

    #[arg(short = 'd', long = "delete", required = false)]
    delete: bool,

    #[arg(short = 'a', long = "acl", required = false, default_value = "private")]
    acl: String,

    #[arg(short = 'u', long = "force-upload", required = false)]
    force_upload: bool,

    #[arg(short = 'm', long = "deploy-manifest", required = false, default_value = ".deploy_manifest.json")]
    deploy_manifest: String,

    #[arg(short = 'c', long = "create-deploy-manifest", required = false)]
    create_deploy_manifest: bool,

    #[arg(short = 'v', long = "verbose", required = false)]
    verbose: bool,

    #[arg(long = "deploy-manifest-acl", required = false, default_value = "private")]
    deploy_manifest_acl: String,

    source: String
}

/* 
   This should create a manifest from local file system, from remote (S3)
   filesystem via list-objects, as well as load up a manifest from the remote
   JSON file, if available. The reason for the JSON manifest is to get remote file
   checksums, avoiding the costly HEAD calls against each object, and ability to
   avoid using local modified time, which is inaccurate when rebuilding a site.
   The two remote manifests can be reconciled first, invalidating checksums if
   the remote modified times differ, etc. Ultimately the local and remote
   manifests are used to determine which files need updloading.
*/

/*
#[derive(Debug, EnumString)]
enum AWSACL {
    #[strum(ascii_case_insensitive, serialize = "private")]
    ObjectCannedAcl::Private,
    #[strum(ascii_case_insensitive, serialize = "public-read")]
    PublicRead,
    #[strum(ascii_case_insensitive, serialize = "public-read-write")]
    PublicReadWrite,
    #[strum(ascii_case_insensitive, serialize = "authenticated-read")]
    AuthenticatedRead,
    #[strum(ascii_case_insensitive, serialize = "aws-exec-read")]
    AWSExecRead,
    #[strum(ascii_case_insensitive, serialize = "bucket-owner-read")]
    BucketOwnerRead,
    #[strum(ascii_case_insensitive, serialize = "bucket-owner-full-control")]
    BucketOwnerFullControl
}
*/

#[::tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CLIArgs::parse();
    
    let root_src_path = fs::canonicalize(&args.source);

    let fs_driver: FSDriver = FSDriver::new(root_src_path.unwrap());
    let fs_manifest = fs_driver.build_manifest().await;

    let s3_driver: S3Driver = S3Driver::new(args.bucket.to_string(), args.prefix.to_string()).await;
    let s3_manifest = s3_driver.build_manifest().await;

    // Define deploy_manifest as mutable. If it doesn't exist, it will be blank
    // and filled in. If it is already defined, it is first used for comparison,
    // and later updated to match new state.
    let mut deploy_manifest: SyncManifest = match s3_driver.get_object_reader(&args.deploy_manifest).await {
        // If transaction log is found, try to parse it.
        Ok(deploy_manifest_result) => {
            match SyncManifest::from_reader(deploy_manifest_result) {
                Ok(manifest) => manifest,
                
                Err(parse_err) => {
                    println!("Could not parse deploy manifest file");
                    println!("{parse_err}");
                    std::process::exit(1);
                }
            }
        },

        // If transaction log was not found, but it should be created, treat it as empty
        Err(_) if args.create_deploy_manifest => SyncManifest::new(),
        
        // If transaction log was expected, but not found, fail.
        Err(_) => {
            println!("Deploy manifest file {} not found", &args.deploy_manifest);
            std::process::exit(1);
        }
    };

    dbg!(&args);

    //update takes care of file creation as well.
    let mut update_files_keys: Vec<&String> = Vec::new();

    for (key, fs_meta) in fs_manifest.files.iter() {
        if args.force_upload {
            update_files_keys.push(key);
            continue;
        }

        match s3_manifest.files.get(key) {
            // New file
            //None => create_files_keys.push(key),
            None => update_files_keys.push(key),

            // Potentially updated file
            Some(s3_meta) => {
                match deploy_manifest.files.get(key) {
                    // Otherwise, update the file
                    None => update_files_keys.push(key),

                    // If we have a deployment record, compare them
                    Some(deploy_meta) => {
                        if deploy_meta.etag == s3_meta.etag && deploy_meta.checksum == fs_meta.checksum {
                            // if the file is indentical all-round, no need to touch it
                            //Nothing to do here
                        } else if deploy_meta.etag != s3_meta.etag || deploy_meta.checksum != fs_meta.checksum {
                            /* Last-modified on S3 does not match the filesystem,
                               and there's no way to make it so. `put_object`
                               does not return last-modified either. The easiest
                               way to detect if files were modified is etag.
                            */

                            // If the s3 and deploy etags mismatch, or deploy and local checksums mismatch, update the file
                            update_files_keys.push(key);
                        
                        } else {
                            panic!("This shouldn't happen... famous last words");
                        }

                    }
                }
            }
        }
    }

    //upload new or updated files
    for item in update_files_keys {
        if args.verbose {
            println!("uploading: {item}");
        }
        
        let mut file_meta = fs_manifest.files.get(item).expect("Key should exist").clone();
        let body = ByteStream::from_path(&file_meta.path).await;

        let ctype: String = file_meta.content_type.as_ref().unwrap().to_string();
        let metadata = HashMap::from([("Content-Type".to_string(), ctype)]);

        let put_result = s3_driver.put_object_data(
            item.to_string(),
            Some(metadata),
            body.unwrap(),
            args.acl.as_str()
        ).await;

        match put_result {
            Ok(result) => file_meta.etag = Some(result.e_tag().expect("Uploaded object should have an e-tag").to_string()),
            Err(_) => file_meta.etag = None,
        }

        deploy_manifest.files.insert(item.to_owned(), file_meta);
    }

    if args.delete {
        let mut delete_files_keys: Vec<String> = Vec::new();

        for key in s3_manifest.files.keys() {
            //TODO: this should be handled at driver level. 
            if key == &args.deploy_manifest {
                continue;
            }

            if !fs_manifest.files.contains_key(key) {
                delete_files_keys.push(key.to_string());
            }
        }

        s3_driver.delete_objects(&delete_files_keys).await;

        for key in delete_files_keys {
            if args.verbose {
                println!("deleting: {key}");
            }

            deploy_manifest.files.remove(&key);
        }
    }
    
    let manifest_json: String = deploy_manifest.to_string().expect("json value");

    let _ = s3_driver.put_object_data(
        (&args.deploy_manifest).to_string(),
        Some(HashMap::from([("Content-Type".to_string(), mime::APPLICATION_JSON.to_string())])),
        ByteStream::from(Bytes::copy_from_slice(manifest_json.as_bytes())),
        args.deploy_manifest_acl.as_str()
    ).await;

    Ok(())
}


