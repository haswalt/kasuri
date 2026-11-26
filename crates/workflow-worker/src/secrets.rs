//! Workspace vault. Only ciphertext crosses Durable Object boundaries.
use crate::{WorkflowWorkspace, api_error, internal};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use connector_sdk::FailureCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::js_sys::{Array, Object, Reflect, Uint8Array, global};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::wasm_bindgen_futures::JsFuture;
use worker::*;

const KEY: &str = "WORKSPACE_SECRETS_KEY";
const LOOKUP_PATH: &str = "/internal/vault/lookup";
const KEY_ERROR: &str = "Workspace secrets need a valid WORKSPACE_SECRETS_KEY Worker secret";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    alias: String,
    origin: String,
    updated_at: u64,
}

#[derive(Serialize, Deserialize)]
struct Sealed {
    nonce: String,
    ciphertext: String,
}

#[derive(Deserialize)]
struct Lookup {
    alias: String,
    origin: String,
}

fn key_bytes(env: &Env) -> Option<Vec<u8>> {
    let key = URL_SAFE_NO_PAD
        .decode(env.secret(KEY).ok()?.to_string().trim())
        .ok()?;
    (key.len() == 32).then_some(key)
}

fn crypto() -> Result<web_sys::Crypto> {
    Reflect::get(&global(), &"crypto".into())?
        .dyn_into::<web_sys::Crypto>()
        .map_err(|_| internal("Web Crypto is unavailable"))
}

async fn crypt(
    crypto: &web_sys::Crypto,
    key: &[u8],
    operation: &str,
    nonce: &[u8],
    aad: &str,
    input: &[u8],
) -> Result<Vec<u8>> {
    let algorithm = Object::new();
    Reflect::set(&algorithm, &"name".into(), &"AES-GCM".into())?;
    let subtle = crypto.subtle();
    let raw_key = Uint8Array::from(key);
    let usage = Array::of1(&JsValue::from_str(operation));
    let key = JsFuture::from(subtle.import_key_with_object(
        "raw",
        raw_key.unchecked_ref(),
        &algorithm,
        false,
        &usage,
    )?)
    .await?
    .dyn_into::<web_sys::CryptoKey>()
    .map_err(|_| internal("Invalid vault key"))?;
    Reflect::set(&algorithm, &"iv".into(), &Uint8Array::from(nonce))?;
    Reflect::set(
        &algorithm,
        &"additionalData".into(),
        &Uint8Array::from(aad.as_bytes()),
    )?;
    let buffer = if operation == "encrypt" {
        subtle.encrypt_with_object_and_u8_array(&algorithm, &key, input)?
    } else {
        subtle.decrypt_with_object_and_u8_array(&algorithm, &key, input)?
    };
    Ok(Uint8Array::new(&JsFuture::from(buffer).await?).to_vec())
}

fn aad(workspace: &str, alias: &str, origin: &str) -> String {
    format!("kasuri-workspace-vault-v1\n{workspace}\n{alias}\n{origin}")
}

fn origin(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

fn table(sql: &SqlStorage) -> Result<()> {
    sql.exec(
        "CREATE TABLE IF NOT EXISTS workspace_vault (
        alias TEXT PRIMARY KEY, origin TEXT NOT NULL, updated_at INTEGER NOT NULL,
        nonce TEXT NOT NULL, ciphertext TEXT NOT NULL
    )",
        None,
    )?;
    Ok(())
}

impl WorkflowWorkspace {
    pub(crate) async fn secret_lookup(&self, mut req: Request) -> Result<Response> {
        let Lookup { alias, origin } = req.json().await?;
        let sql = self.state.storage().sql();
        table(&sql)?;
        let sealed = sql
            .exec(
                "SELECT nonce, ciphertext FROM workspace_vault WHERE alias = ? AND origin = ?",
                vec![alias.into(), origin.into()],
            )?
            .to_array::<Sealed>()?;
        match sealed.first() {
            Some(row) => Response::from_json(row),
            None => api_error(404, "No secret for this alias and destination"),
        }
    }

    pub(crate) async fn secret_management(
        &self,
        path: &str,
        method: Method,
        body: &[u8],
        actor: &str,
    ) -> Result<Response> {
        let sql = self.state.storage().sql();
        table(&sql)?;
        let key = key_bytes(&self.env);
        if path == "/api/secrets" && method == Method::Get {
            let secrets = sql.exec(
                "SELECT alias, origin, updated_at AS updatedAt FROM workspace_vault ORDER BY alias",
                None,
            )?.to_array::<Metadata>()?;
            return Response::from_json(&json!({"configured":key.is_some(),"secrets":secrets}));
        }
        let Some(alias) = path.strip_prefix("/api/secrets/").filter(|alias| {
            connectors_http::valid_secret_name(alias) && !matches!(*alias, "." | "..")
        }) else {
            return api_error(404, "Secret not found");
        };
        let existing = sql
            .exec(
                "SELECT origin, nonce FROM workspace_vault WHERE alias = ?",
                vec![alias.into()],
            )?
            .to_array::<serde_json::Value>()?;
        let existing_origin = existing.first().and_then(|row| row["origin"].as_str());
        let timestamp = crate::now();
        if method == Method::Delete {
            if existing_origin.is_none() {
                return api_error(404, "Secret not found");
            }
            let mut workspace = self.load()?;
            workspace.record_audit(actor, "secret.deleted", alias, "succeeded", timestamp, "");
            let alias = alias.to_owned();
            self.save_with_sql(&mut workspace, move |sql| {
                sql.exec(
                    "DELETE FROM workspace_vault WHERE alias = ?",
                    vec![alias.into()],
                )?;
                Ok(())
            })
            .await?;
            return Response::from_json(&json!({"deleted":true}));
        }
        if !matches!(method, Method::Post | Method::Put) {
            return api_error(405, "Method not allowed");
        }
        let Some(key) = key else {
            return api_error(503, KEY_ERROR);
        };
        let input: serde_json::Value = match serde_json::from_slice(body) {
            Ok(input) => input,
            Err(_) => return api_error(400, "Expected a secret value"),
        };
        let Some(value) = input["value"]
            .as_str()
            .filter(|value| !value.is_empty() && connectors_http::valid_header_value(value))
        else {
            return api_error(
                400,
                "Enter a non-empty HTTP credential without control characters",
            );
        };
        let (origin, action) = if method == Method::Post {
            if existing_origin.is_some() {
                return api_error(409, "Secret alias already exists");
            }
            let Some(origin) = input["origin"].as_str().and_then(origin) else {
                return api_error(
                    400,
                    "Enter an HTTPS origin without a path, query or fragment",
                );
            };
            (origin, "secret.created")
        } else {
            let Some(origin) = existing_origin else {
                return api_error(404, "Secret not found");
            };
            (origin.to_owned(), "secret.rotated")
        };
        let crypto = crypto()?;
        let mut nonce = [0_u8; 12];
        crypto.get_random_values_with_u8_array(&mut nonce)?;
        let ciphertext = crypt(
            &crypto,
            &key,
            "encrypt",
            &nonce,
            &aad(&self.state.id().to_string(), alias, &origin),
            value.as_bytes(),
        )
        .await
        .map_err(|_| internal("Could not encrypt workspace secret"))?;
        // Encryption yields. Recheck existence and destination before committing to avoid
        // overwriting a concurrent create or restoring an alias deleted in the meantime.
        let current = sql
            .exec(
                "SELECT origin, nonce FROM workspace_vault WHERE alias = ?",
                vec![alias.into()],
            )?
            .to_array::<serde_json::Value>()?;
        if existing != current {
            return api_error(409, "Secret changed while saving. Refresh and retry.");
        }
        let metadata = Metadata {
            alias: alias.to_owned(),
            origin: origin.clone(),
            updated_at: timestamp,
        };
        let mut workspace = self.load()?;
        workspace.record_audit(actor, action, alias, "succeeded", timestamp, "");
        let alias = alias.to_owned();
        self.save_with_sql(&mut workspace, move |sql| {
            sql.exec(
                "INSERT INTO workspace_vault (alias, origin, updated_at, nonce, ciphertext) VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(alias) DO UPDATE SET updated_at = excluded.updated_at, nonce = excluded.nonce, ciphertext = excluded.ciphertext",
                vec![alias.into(), origin.into(), (timestamp as i64).into(), URL_SAFE_NO_PAD.encode(nonce).into(), URL_SAFE_NO_PAD.encode(ciphertext).into()],
            )?;
            Ok(())
        }).await?;
        Response::from_json(&metadata)
    }
}

/// No public route forwards this path. Scope the lookup to the executing workspace
/// and destination. Only the host decrypts, after the run has saved its dispatch.
pub(crate) async fn resolve(
    env: &Env,
    workspace: &str,
    lookup_origin: &str,
    alias: &str,
) -> std::result::Result<String, FailureCode> {
    let key = key_bytes(env).ok_or(FailureCode::MissingInput)?;
    let load = async {
        let namespace = env.durable_object("WORKSPACES")?;
        let stub = namespace.id_from_string(workspace)?.get_stub()?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post).with_body(Some(
            serde_json::to_string(&json!({"alias":alias,"origin":lookup_origin}))?.into(),
        ));
        let mut response = stub
            .fetch_with_request(Request::new_with_init(
                &format!("https://workspace{LOOKUP_PATH}"),
                &init,
            )?)
            .await?;
        if response.status_code() == 404 {
            return Ok(None);
        }
        if response.status_code() != 200 {
            return Err(internal("Secret lookup failed"));
        }
        let Sealed { nonce, ciphertext } = response.json().await?;
        let nonce = URL_SAFE_NO_PAD.decode(nonce).map_err(internal)?;
        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext).map_err(internal)?;
        let decrypted = crypt(
            &crypto()?,
            &key,
            "decrypt",
            &nonce,
            &aad(workspace, alias, lookup_origin),
            &ciphertext,
        )
        .await?;
        Ok::<_, Error>(Some(String::from_utf8(decrypted).map_err(internal)?))
    };
    load.await
        .map_err(|_| FailureCode::MissingInput)?
        .ok_or(FailureCode::EgressDenied)
}

pub(crate) fn is_lookup(path: &str, method: Method) -> bool {
    path == LOOKUP_PATH && method == Method::Post
}
