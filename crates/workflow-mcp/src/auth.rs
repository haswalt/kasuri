use crate::SCOPES;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;

/// Lowercase hex of a digest, matching the `{:x}` format of sha2 0.10 output.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn hash(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}
fn same(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}
pub fn valid_scopes(scopes: &[String]) -> bool {
    !scopes.is_empty()
        && scopes.len() <= SCOPES.len()
        && scopes.iter().all(|s| SCOPES.contains(&s.as_str()))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    pub issuer: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub resource: String,
    pub token_hash: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub revoked: bool,
    pub client_id: Option<String>,
    #[serde(default)]
    pub authorized_by: Option<UserIdentity>,
    pub refresh_hash: Option<String>,
    pub refresh_until: u64,
    #[serde(default)]
    pub used_refresh: Vec<String>,
    #[serde(default)]
    pub minute: u64,
    #[serde(default)]
    pub calls: u32,
}
impl Credential {
    pub fn permits(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
    pub fn public(&self) -> Value {
        json!({"id":self.id,"name":self.name,"scopes":self.scopes,"createdAt":self.created_at,"expiresAt":self.expires_at,"revoked":self.revoked,"clientId":self.client_id,"authentication":if self.client_id.is_some(){"oauth"}else{"token"}})
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub redirects: Vec<String>,
    pub created: u64,
    /// Set once the client completes an authorization. Clients stored before this
    /// field existed are treated as established, keeping their original retention.
    #[serde(default = "established_default")]
    pub established: bool,
}
fn established_default() -> bool {
    true
}
/// Registration is unauthenticated, so clients that never complete an authorization
/// are kept only briefly and are evicted first when the registry is full.
const UNUSED_CLIENT_MS: u64 = 3_600_000;
const ESTABLISHED_CLIENT_MS: u64 = 90 * 86_400_000;
const MAX_CLIENTS: usize = 128;
const MAX_REQUESTS: usize = 64;
#[derive(Clone, Serialize, Deserialize)]
pub struct GrantRequest {
    pub id: String,
    pub client_id: String,
    pub redirect: String,
    pub resource: String,
    pub challenge: String,
    pub state: String,
    pub scopes: Vec<String>,
    pub expires: u64,
    pub code_hash: Option<String>,
    pub approved: Option<Vec<String>>,
    #[serde(default)]
    pub authorized_by: Option<UserIdentity>,
    pub used: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub actor: String,
    pub key: String,
    pub fingerprint: String,
    pub result: Value,
    pub expires: u64,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Auth {
    #[serde(default)]
    pub credentials: Vec<Credential>,
    #[serde(default)]
    pub clients: Vec<Client>,
    #[serde(default)]
    pub requests: Vec<GrantRequest>,
    #[serde(default)]
    pub receipts: Vec<Receipt>,
}
impl Auth {
    pub fn prune(&mut self, now: u64) {
        self.requests.retain(|r| r.expires > now);
        self.receipts.retain(|r| r.expires > now);
        self.credentials
            .retain(|c| c.expires_at.max(c.refresh_until) > now);
        let (credentials, requests) = (&self.credentials, &self.requests);
        self.clients.retain(|c| {
            let lifetime = if c.established {
                ESTABLISHED_CLIENT_MS
            } else {
                UNUSED_CLIENT_MS
            };
            c.created.saturating_add(lifetime) > now
                || credentials
                    .iter()
                    .any(|g| g.client_id.as_deref() == Some(&c.id))
                || requests.iter().any(|r| r.client_id == c.id)
        });
    }
    /// The oldest client that never completed an authorization and has no live
    /// credential or request, if any.
    fn evictable_client(&self) -> Option<usize> {
        self.clients
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                !c.established
                    && !self
                        .credentials
                        .iter()
                        .any(|g| g.client_id.as_deref() == Some(&c.id))
                    && !self.requests.iter().any(|r| r.client_id == c.id)
            })
            .min_by_key(|(_, c)| c.created)
            .map(|(i, _)| i)
    }
    pub fn authenticate(
        &mut self,
        token: &str,
        resource: &str,
        now: u64,
    ) -> Result<Credential, &'static str> {
        if token.len() > 256 || !token.starts_with("kasuri_mcp_") {
            return Err("invalid_token");
        }
        let digest = hash(token);
        let c = self
            .credentials
            .iter_mut()
            .find(|c| {
                same(&c.token_hash, &digest)
                    && !c.revoked
                    && c.expires_at > now
                    && c.resource == resource
            })
            .ok_or("invalid_token")?;
        let minute = now / 60000;
        if c.minute != minute {
            c.minute = minute;
            c.calls = 0
        }
        if c.calls >= 120 {
            return Err("rate_limited");
        };
        c.calls += 1;
        Ok(c.clone())
    }
    pub fn issue(
        &mut self,
        name: &str,
        scopes: Vec<String>,
        resource: &str,
        now: u64,
        lifetime: u64,
        entropy: &mut dyn FnMut() -> String,
    ) -> Result<Value, &'static str> {
        self.prune(now);
        if self.credentials.len() >= 64 {
            return Err("credential_limit");
        }
        if name.trim().is_empty()
            || name.len() > 80
            || !valid_scopes(&scopes)
            || !(3600000..=90 * 86400000).contains(&lifetime)
        {
            return Err("invalid_grant");
        }
        let token = format!("kasuri_mcp_{}", entropy());
        let c = Credential {
            id: entropy(),
            name: name.trim().into(),
            scopes,
            resource: resource.into(),
            token_hash: hash(&token),
            created_at: now,
            expires_at: now + lifetime,
            revoked: false,
            client_id: None,
            authorized_by: None,
            refresh_hash: None,
            refresh_until: 0,
            used_refresh: vec![],
            minute: 0,
            calls: 0,
        };
        let mut value = c.public();
        value["token"] = json!(token);
        self.credentials.push(c);
        Ok(value)
    }
    pub fn register(
        &mut self,
        input: &Value,
        now: u64,
        entropy: &mut dyn FnMut() -> String,
    ) -> Result<Value, &'static str> {
        self.prune(now);
        if self.clients.len() >= MAX_CLIENTS {
            let index = self.evictable_client().ok_or("client_limit")?;
            self.clients.remove(index);
        }
        let redirects = input["redirect_uris"]
            .as_array()
            .filter(|v| !v.is_empty() && v.len() <= 8)
            .ok_or("invalid_redirect_uri")?;
        let mut urls = vec![];
        for value in redirects {
            let raw = value.as_str().ok_or("invalid_redirect_uri")?;
            let url = Url::parse(raw).map_err(|_| "invalid_redirect_uri")?;
            if raw.len() > 2048
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.host_str().is_none()
                || !(url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
            {
                return Err("invalid_redirect_uri");
            };
            urls.push(raw.to_string());
        }
        if input["token_endpoint_auth_method"]
            .as_str()
            .unwrap_or("none")
            != "none"
        {
            return Err("invalid_client_metadata");
        }
        let name = input["client_name"].as_str().unwrap_or("MCP client");
        if name.len() > 80 {
            return Err("invalid_client_metadata");
        }
        let c = Client {
            id: entropy(),
            name: name.into(),
            redirects: urls,
            created: now,
            established: false,
        };
        let result = json!({"client_id":c.id,"client_name":c.name,"redirect_uris":c.redirects,"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]});
        self.clients.push(c);
        Ok(result)
    }
    pub fn authorize(
        &mut self,
        input: &Value,
        resource: &str,
        now: u64,
        entropy: &mut dyn FnMut() -> String,
    ) -> Result<String, &'static str> {
        self.prune(now);
        if self.requests.len() >= MAX_REQUESTS {
            // Drop the oldest request still awaiting consent. Approved requests are
            // kept so a user who has already consented can finish the exchange.
            let index = self
                .requests
                .iter()
                .enumerate()
                .filter(|(_, r)| r.approved.is_none() && !r.used)
                .min_by_key(|(_, r)| r.expires)
                .map(|(i, _)| i)
                .ok_or("request_limit")?;
            self.requests.remove(index);
        }
        let get = |key| input[key].as_str().unwrap_or("");
        let c = self
            .clients
            .iter()
            .find(|c| c.id == get("client_id"))
            .ok_or("invalid_client")?;
        let scopes: Vec<String> = if get("scope").is_empty() {
            crate::role("reader")
        } else {
            get("scope")
                .split_whitespace()
                .map(str::to_string)
                .collect()
        };
        if !c.redirects.iter().any(|r| r == get("redirect_uri")) {
            return Err("invalid_redirect_uri");
        }
        if get("response_type") != "code"
            || get("resource") != resource
            || get("code_challenge_method") != "S256"
            || get("code_challenge").len() != 43
            || URL_SAFE_NO_PAD
                .decode(get("code_challenge"))
                .ok()
                .is_none_or(|v| v.len() != 32)
            || get("state").is_empty()
            || get("state").len() > 1024
            || !valid_scopes(&scopes)
        {
            return Err("invalid_request");
        }
        let id = entropy();
        self.requests.push(GrantRequest {
            id: id.clone(),
            client_id: c.id.clone(),
            redirect: get("redirect_uri").into(),
            resource: resource.into(),
            challenge: get("code_challenge").into(),
            state: get("state").into(),
            scopes,
            expires: now + 600000,
            code_hash: None,
            approved: None,
            authorized_by: None,
            used: false,
        });
        Ok(id)
    }
    pub fn consent_info(&self, id: &str, now: u64) -> Result<Value, &'static str> {
        let r = self
            .requests
            .iter()
            .find(|r| r.id == id && r.expires > now && !r.used && r.approved.is_none())
            .ok_or("invalid_request")?;
        let client = self
            .clients
            .iter()
            .find(|c| c.id == r.client_id)
            .ok_or("invalid_client")?;
        Ok(
            json!({"id":r.id,"clientName":client.name,"clientId":client.id,"redirectUri":r.redirect,"scopes":r.scopes,"resource":r.resource}),
        )
    }
    pub fn consent(
        &mut self,
        id: &str,
        scopes: Vec<String>,
        now: u64,
        entropy: &mut dyn FnMut() -> String,
    ) -> Result<String, &'static str> {
        if !valid_scopes(&scopes) {
            return Err("invalid_scope");
        }
        let r = self
            .requests
            .iter_mut()
            .find(|r| r.id == id && r.expires > now && !r.used && r.approved.is_none())
            .ok_or("invalid_request")?;
        if !scopes.iter().all(|s| r.scopes.contains(s)) {
            return Err("invalid_scope");
        }
        let code = entropy();
        r.code_hash = Some(hash(&code));
        r.approved = Some(scopes);
        r.expires = now + 120000;
        let mut redirect = Url::parse(&r.redirect).map_err(|_| "invalid_redirect_uri")?;
        redirect
            .query_pairs_mut()
            .append_pair("code", &code)
            .append_pair("state", &r.state);
        Ok(redirect.to_string())
    }
    pub fn exchange(
        &mut self,
        input: &Value,
        resource: &str,
        now: u64,
        entropy: &mut dyn FnMut() -> String,
    ) -> Result<Value, &'static str> {
        let get = |key| input[key].as_str().unwrap_or("");
        if get("resource") != resource
            && !(get("grant_type") == "refresh_token" && get("resource").is_empty())
        {
            return Err("invalid_target");
        }
        if get("grant_type") == "refresh_token" {
            let digest = hash(get("refresh_token"));
            if let Some(c) = self
                .credentials
                .iter_mut()
                .find(|c| c.used_refresh.contains(&digest))
            {
                c.revoked = true;
                return Err("invalid_grant");
            }
            let c = self
                .credentials
                .iter_mut()
                .find(|c| {
                    !c.revoked
                        && c.refresh_until > now
                        && c.resource == resource
                        && c.client_id.as_deref() == Some(get("client_id"))
                        && c.refresh_hash.as_ref().is_some_and(|h| same(h, &digest))
                })
                .ok_or("invalid_grant")?;
            let scope: Vec<String> = if get("scope").is_empty() {
                c.scopes.clone()
            } else {
                get("scope")
                    .split_whitespace()
                    .map(str::to_string)
                    .collect()
            };
            if !valid_scopes(&scope) || !scope.iter().all(|s| c.scopes.contains(s)) {
                return Err("invalid_scope");
            };
            c.scopes = scope;
            c.used_refresh.push(digest);
            if c.used_refresh.len() > 32 {
                c.used_refresh.remove(0);
            }
            let access = format!("kasuri_mcp_{}", entropy());
            let refresh = entropy();
            c.token_hash = hash(&access);
            c.refresh_hash = Some(hash(&refresh));
            c.expires_at = now + 3600000;
            return Ok(
                json!({"access_token":access,"refresh_token":refresh,"token_type":"Bearer","expires_in":3600,"scope":c.scopes.join(" ")}),
            );
        }
        if get("grant_type") != "authorization_code" {
            return Err("unsupported_grant_type");
        }
        let digest = hash(get("code"));
        let verifier = get("code_verifier");
        let r = self
            .requests
            .iter_mut()
            .find(|r| {
                !r.used
                    && r.expires > now
                    && r.client_id == get("client_id")
                    && r.resource == resource
                    && r.redirect == get("redirect_uri")
                    && r.code_hash.as_ref().is_some_and(|h| same(h, &digest))
            })
            .ok_or("invalid_grant")?;
        if !(43..=128).contains(&verifier.len())
            || !verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
            || !same(
                &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
                &r.challenge,
            )
        {
            return Err("invalid_grant");
        }
        let scopes = r.approved.clone().ok_or("invalid_grant")?;
        let client_id = r.client_id.clone();
        let authorized_by = r.authorized_by.clone();
        r.used = true;
        if let Some(client) = self.clients.iter_mut().find(|c| c.id == client_id) {
            client.established = true;
        }
        let name = self
            .clients
            .iter()
            .find(|c| c.id == client_id)
            .map(|c| c.name.clone())
            .unwrap_or("OAuth client".into());
        let result = self.issue(&name, scopes, resource, now, 3600000, entropy)?;
        let refresh = entropy();
        let c = self.credentials.last_mut().unwrap();
        c.client_id = Some(client_id);
        c.authorized_by = authorized_by;
        c.refresh_hash = Some(hash(&refresh));
        c.refresh_until = now + 30 * 86400000;
        Ok(
            json!({"access_token":result["token"],"refresh_token":refresh,"token_type":"Bearer","expires_in":3600,"scope":c.scopes.join(" ")}),
        )
    }
}
