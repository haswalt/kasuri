//! Private R2 attachment storage. The runtime supplies the workspace key prefix.
use crate::internal;
use connector_sdk::FailureCode;
use connectors_clairic::MediaPart;
use worker::{Bucket, Env, HttpMetadata, Result};
use workflow_files::{
    FileReference, MAX_FILE_BYTES, MAX_MEDIA_BYTES, MAX_MEDIA_ITEMS, detect_media, valid_id,
};

fn key(workspace: &str, id: &str) -> String {
    format!("{workspace}/files/{id}")
}

pub(crate) async fn store_attachments(
    env: &Env,
    workspace: &str,
    mail: &connectors_email::ParsedMessage<'_>,
) -> Result<(Vec<FileReference>, usize)> {
    let file_count = mail.attachments().count();
    if file_count == 0 {
        return Ok((Vec::new(), 0));
    }
    let Ok(bucket) = env.bucket("FILES") else {
        return Ok((Vec::new(), file_count));
    };
    let mut stored = Vec::new();
    let mut bytes_stored = 0;
    for part in mail.attachments() {
        if stored.len() == MAX_MEDIA_ITEMS {
            break;
        }
        let size = part.decoded_len() as u64;
        if size > MAX_FILE_BYTES || bytes_stored + size > MAX_MEDIA_BYTES {
            continue;
        }
        let bytes = part.decoded();
        let Some(content_type) = detect_media(&bytes) else {
            continue;
        };
        let id = env.durable_object("WORKSPACES")?.unique_id()?.to_string();
        let mut name = String::new();
        for ch in part
            .attachment_name()
            .unwrap_or_default()
            .chars()
            .filter(|ch| !ch.is_control())
        {
            if name.len() + ch.len_utf8() > 255 {
                break;
            }
            name.push(ch);
        }
        if name.trim().is_empty() {
            name = "attachment".into();
        }
        let reference = FileReference {
            id,
            name,
            size: bytes.len() as u64,
            content_type: content_type.into(),
        };
        if reference.size > MAX_FILE_BYTES || bytes_stored + reference.size > MAX_MEDIA_BYTES {
            continue;
        }
        let saved = bucket
            .put(key(workspace, &reference.id), bytes.into_owned())
            .http_metadata(HttpMetadata {
                content_type: Some(reference.content_type.clone()),
                ..Default::default()
            })
            .execute()
            .await;
        if !matches!(saved, Ok(Some(_))) {
            discard(&bucket, workspace, &stored).await;
            return Err(internal("Could not store email attachments"));
        }
        bytes_stored += reference.size;
        stored.push(reference);
    }
    let skipped = file_count.saturating_sub(stored.len());
    Ok((stored, skipped))
}

pub(crate) async fn discard(bucket: &Bucket, workspace: &str, files: &[FileReference]) {
    if !files.is_empty() {
        let keys: Vec<String> = files.iter().map(|file| key(workspace, &file.id)).collect();
        // A bucket lifecycle also covers interrupted deliveries and failed cleanup.
        let _: Result<()> = bucket.delete_multiple(keys).await;
    }
}

pub(crate) async fn load_media(
    env: &Env,
    workspace: &str,
    files: &[FileReference],
) -> std::result::Result<Vec<MediaPart>, FailureCode> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    if !valid_id(workspace) {
        return Err(FailureCode::InvalidConfig);
    }
    let bucket = env
        .bucket("FILES")
        .map_err(|_| FailureCode::InvalidConfig)?;
    let mut media = Vec::with_capacity(files.len());
    for reference in files {
        let object = bucket
            .get(key(workspace, &reference.id))
            .execute()
            .await
            .map_err(|_| FailureCode::External)?
            .ok_or(FailureCode::MissingInput)?;
        if object.size() != reference.size
            || object.http_metadata().content_type.as_deref() != Some(&reference.content_type)
        {
            return Err(FailureCode::InvalidConfig);
        }
        let bytes = object
            .body()
            .ok_or(FailureCode::MissingInput)?
            .bytes()
            .await
            .map_err(|_| FailureCode::External)?;
        if bytes.len() as u64 != reference.size
            || detect_media(&bytes) != Some(&reference.content_type)
        {
            return Err(FailureCode::InvalidConfig);
        }
        use base64::{Engine, engine::general_purpose::STANDARD};
        media.push(MediaPart {
            content_type: reference.content_type.clone(),
            base64: STANDARD.encode(bytes),
        });
    }
    Ok(media)
}
