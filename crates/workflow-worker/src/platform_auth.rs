//! Browser authentication. Only the public Worker verifies assertions and forwards principals.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap};
use worker::wasm_bindgen::{JsCast, JsValue};
use worker::wasm_bindgen_futures::JsFuture;
use worker::*;
use workflow_mcp::auth::UserIdentity;

pub(crate) const PRINCIPAL_HEADER: &str = "X-Kasuri-Principal";

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Principal(pub(crate) UserIdentity);

impl Principal {
    pub(crate) fn account_key(&self) -> String {
        workflow_mcp::auth::hash(&serde_json::json!([self.0.issuer, self.0.subject]).to_string())
    }

    pub(crate) fn directory<'a>(
        &self,
        env: &Env,
        namespace: &'a ObjectNamespace,
    ) -> Result<ObjectId<'a>> {
        let raw = env
            .secret("PLATFORM_ACCESS_LEGACY_OWNERS")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let owners: BTreeMap<String, String> = if raw.trim().is_empty() {
            BTreeMap::new()
        } else {
            serde_json::from_str(&raw).map_err(crate::internal)?
        };
        let mut assigned = std::collections::BTreeSet::new();
        for id in owners.values() {
            if !assigned.insert(id) || namespace.id_from_string(id).is_err() {
                return Err(crate::internal(
                    "Invalid or duplicate legacy workspace mapping",
                ));
            }
        }
        match owners.get(&self.0.subject) {
            Some(id) => namespace.id_from_string(id),
            None => namespace.id_from_name(&format!("account:{}", self.account_key())),
        }
    }
}

pub(crate) fn principal(req: &Request) -> Result<Option<Principal>> {
    req.headers()
        .get(PRINCIPAL_HEADER)?
        .map(|value| {
            let bytes = URL_SAFE_NO_PAD.decode(value).map_err(crate::internal)?;
            serde_json::from_slice(&bytes).map_err(crate::internal)
        })
        .transpose()
}

pub(crate) enum BrowserAuth {
    Principal(Principal),
    Local,
}

#[derive(Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    email: Option<String>,
    service_token_status: Option<bool>,
    aud: Vec<String>,
    exp: u64,
    nbf: Option<u64>,
    #[serde(rename = "type")]
    assertion_type: String,
}

struct KeySet {
    issuer: String,
    loaded_at: u64,
    keys: Vec<Value>,
}

thread_local! {
    static KEY_SET: RefCell<Option<KeySet>> = const { RefCell::new(None) };
}

async fn key(issuer: &str, kid: &str, now: u64) -> Option<Value> {
    let cached = KEY_SET.with_borrow(|stored| {
        stored
            .as_ref()
            .filter(|keys| keys.issuer == issuer && now.saturating_sub(keys.loaded_at) < 300)
            .map(|keys| {
                (
                    keys.keys.iter().find(|key| key["kid"] == kid).cloned(),
                    now.saturating_sub(keys.loaded_at),
                )
            })
    });
    match cached {
        Some((Some(key), _)) => return Some(key),
        Some((None, age)) if age < 30 => return None,
        _ => {}
    }
    let mut init = RequestInit::new();
    init.with_redirect(RequestRedirect::Manual);
    let request = Request::new_with_init(&format!("{issuer}/cdn-cgi/access/certs"), &init).ok()?;
    use futures_util::future::{Either, select};
    let controller = AbortController::default();
    let signal = controller.signal();
    let fetch = Box::pin(async {
        let mut response = Fetch::Request(request)
            .send_with_signal(&signal)
            .await
            .ok()?;
        if response.status_code() != 200 {
            return None;
        }
        let mut bytes = Vec::new();
        let mut stream = response.stream().ok()?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.ok()?;
            if bytes.len() + chunk.len() > 65_536 {
                return None;
            }
            bytes.extend(chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).ok()?;
        value["keys"].as_array().cloned()
    });
    let timeout = Box::pin(Delay::from(std::time::Duration::from_secs(5)));
    let keys = match select(fetch, timeout).await {
        Either::Left((keys, _)) => keys,
        Either::Right(_) => None,
    };
    controller.abort();
    let keys = keys?;
    let key = keys.iter().find(|key| key["kid"] == kid).cloned();
    KEY_SET.with_borrow_mut(|stored| {
        *stored = Some(KeySet {
            issuer: issuer.into(),
            loaded_at: now,
            keys,
        })
    });
    key
}

async fn verify(assertion: &str, issuer: &str, audience: &str) -> Option<Principal> {
    if assertion.len() > 16_384 {
        return None;
    }
    let (signed, encoded_signature) = assertion.rsplit_once('.')?;
    let (encoded_header, encoded_claims) = signed.split_once('.')?;
    let header: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded_header).ok()?).ok()?;
    let kid = header["kid"].as_str()?;
    if header["alg"] != "RS256" || header.get("crit").is_some() || kid.is_empty() {
        return None;
    }
    let claims: Claims =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded_claims).ok()?).ok()?;
    let now = crate::now() / 1000;
    if claims.iss != issuer
        || !claims.aud.iter().any(|aud| aud == audience)
        || claims.exp <= now
        || claims.nbf.is_some_and(|nbf| nbf > now)
        || claims.assertion_type != "app"
        || claims.service_token_status == Some(true)
        || claims.sub.is_empty()
        || claims.sub.len() > 256
        || claims.sub.chars().any(char::is_control)
    {
        return None;
    }
    let signature = URL_SAFE_NO_PAD.decode(encoded_signature).ok()?;
    let jwk = key(issuer, kid, now).await?;
    if jwk["kty"] != "RSA" || jwk["alg"].as_str().is_some_and(|alg| alg != "RS256") {
        return None;
    }

    // Workers' Web Crypto performs the RS256 verification over the original compact JWT bytes.
    use worker::js_sys::{Array, JSON, Reflect, Uint8Array, global};
    let crypto = Reflect::get(&global(), &"crypto".into())
        .ok()?
        .dyn_into::<web_sys::Crypto>()
        .ok()?;
    let algorithm = JSON::parse(r#"{"name":"RSASSA-PKCS1-v1_5","hash":"SHA-256"}"#).ok()?;
    let data = JSON::parse(&jwk.to_string()).ok()?;
    let usage = Array::of1(&JsValue::from_str("verify"));
    let imported = crypto
        .subtle()
        .import_key_with_object(
            "jwk",
            data.unchecked_ref(),
            algorithm.unchecked_ref(),
            false,
            &usage,
        )
        .ok()?;
    let imported = JsFuture::from(imported)
        .await
        .ok()?
        .dyn_into::<web_sys::CryptoKey>()
        .ok()?;
    let signature = Uint8Array::from(signature.as_slice());
    let signed = Uint8Array::from(signed.as_bytes());
    let verified = crypto
        .subtle()
        .verify_with_str_and_buffer_source_and_buffer_source(
            "RSASSA-PKCS1-v1_5",
            &imported,
            signature.unchecked_ref(),
            signed.unchecked_ref(),
        )
        .ok()?;
    if JsFuture::from(verified).await.ok()?.as_bool() != Some(true) {
        return None;
    }
    Some(Principal(UserIdentity {
        issuer: claims.iss,
        subject: claims.sub,
        email: claims.email.filter(|email| {
            !email.is_empty() && email.len() <= 320 && !email.chars().any(char::is_control)
        }),
    }))
}

pub(crate) async fn authenticate(
    req: &Request,
    env: &Env,
    url: &Url,
) -> Result<std::result::Result<BrowserAuth, Response>> {
    let mode = env
        .var("PLATFORM_AUTH")
        .map(|value| value.to_string())
        .unwrap_or_default();
    if mode == "local" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")) {
        return Ok(Ok(BrowserAuth::Local));
    }
    let issuer = env
        .var("PLATFORM_ACCESS_ISSUER")
        .map(|value| value.to_string())
        .unwrap_or_default();
    let audience = env
        .var("PLATFORM_ACCESS_AUDIENCE")
        .map(|value| value.to_string())
        .unwrap_or_default();
    if (!mode.is_empty() && mode != "access")
        || audience.is_empty()
        || Url::parse(&issuer).ok().is_none_or(|url| {
            url.scheme() != "https"
                || url.origin().ascii_serialization() != issuer
                || !url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".cloudflareaccess.com"))
        })
    {
        return Ok(Err(crate::api_error(
            503,
            "Platform sign-in is not configured",
        )?));
    }
    if let Some(jwt) = req.headers().get("Cf-Access-Jwt-Assertion")? {
        if let Some(principal) = verify(&jwt, &issuer, &audience).await {
            return Ok(Ok(BrowserAuth::Principal(principal)));
        }
    }
    Ok(Err(crate::api_error(
        401,
        "Sign in through Cloudflare Access",
    )?))
}

pub(crate) fn forward(req: &mut Request, principal: &Principal) -> Result<()> {
    let value = serde_json::to_vec(principal).map_err(crate::internal)?;
    req.headers_mut()?
        .set(PRINCIPAL_HEADER, &URL_SAFE_NO_PAD.encode(value))
}
