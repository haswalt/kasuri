//! Rust HTTP binding for scoped MCP, OAuth discovery and owner consent.
use crate::{WorkflowWorkspace, api_error, internal, now};
use futures_util::StreamExt;
use serde_json::{Value, json};
use worker::*;
use workflow_mcp::{
    SCOPES,
    auth::{Auth, hash},
};
use workflow_service::MAX_BODY_BYTES;

pub(crate) fn public_workspace(path: &str) -> Option<&str> {
    let p: Vec<_> = path.trim_matches('/').split('/').collect();
    match p.as_slice() {
        ["mcp", id, "mcp"] | ["oauth", id, "register" | "authorize" | "token" | "revoke"] => {
            Some(id)
        }
        [".well-known", "oauth-protected-resource", "mcp", id, "mcp"]
        | [".well-known", "oauth-authorization-server", "oauth", id] => Some(id),
        _ => None,
    }
}
async fn body(req: &mut Request) -> Result<Option<Vec<u8>>> {
    let mut data = vec![];
    let mut stream = req.stream()?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if data.len() + chunk.len() > MAX_BODY_BYTES {
            return Ok(None);
        }
        data.extend(chunk)
    }
    Ok(Some(data))
}
fn json(value: &Value, status: u16) -> Result<Response> {
    let mut r = Response::from_json(value)?.with_status(status);
    r.headers_mut().set("Cache-Control", "no-store")?;
    r.headers_mut().set("X-Content-Type-Options", "nosniff")?;
    Ok(r)
}
fn oauth_error(error: &str) -> Result<Response> {
    json(&json!({"error":error}), 400)
}
fn form(bytes: &[u8]) -> Value {
    Value::Object(
        url::form_urlencoded::parse(bytes)
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    )
}
impl WorkflowWorkspace {
    fn mcp_public_budget(&self, req: &Request, path: &str) -> Result<bool> {
        if path.starts_with("/.well-known/") {
            return Ok(true);
        }
        let minute = now() / 60000;
        let category = if path.ends_with("/register") {
            "register"
        } else if path.starts_with("/mcp/") {
            "rpc"
        } else {
            "oauth"
        };
        let limit = match category {
            "register" => 8,
            "rpc" => 500,
            _ => 60,
        };
        let peer = req
            .headers()
            .get("CF-Connecting-IP")?
            .unwrap_or_else(|| "shared".into());
        let key = hash(&format!("{category}:{minute}:{peer}"));
        let sql = self.state.storage().sql();
        sql.exec("CREATE TABLE IF NOT EXISTS mcp_rate (bucket TEXT PRIMARY KEY, count INTEGER NOT NULL, expires INTEGER NOT NULL)",None)?;
        sql.exec(
            "DELETE FROM mcp_rate WHERE expires < ?",
            Some(vec![(minute as f64).into()]),
        )?;
        #[derive(serde::Deserialize)]
        struct Count {
            count: u32,
        }
        let rows = sql
            .exec(
                "SELECT count FROM mcp_rate WHERE bucket=?",
                Some(vec![key.clone().into()]),
            )?
            .to_array::<Count>()?;
        if rows.first().is_some_and(|r| r.count >= limit) {
            return Ok(false);
        }
        let size = sql
            .exec("SELECT count(*) AS count FROM mcp_rate", None)?
            .one::<Count>()?;
        if rows.is_empty() && size.count >= 2048 {
            return Ok(false);
        }
        sql.exec("INSERT INTO mcp_rate(bucket,count,expires) VALUES(?,1,?) ON CONFLICT(bucket) DO UPDATE SET count=count+1",Some(vec![key.into(),((minute+2) as f64).into()]))?;
        Ok(true)
    }
    fn mcp_origin(&self, url: &Url) -> Result<String> {
        let configured = self
            .env
            .var("PUBLIC_ORIGIN")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let origin = if configured.is_empty() {
            url.origin().ascii_serialization()
        } else {
            configured
        };
        let parsed = Url::parse(&origin).map_err(internal)?;
        if parsed.origin().ascii_serialization() != origin
            || !(parsed.scheme() == "https"
                || (parsed.scheme() == "http"
                    && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))))
        {
            return Err(internal(
                "Set PUBLIC_ORIGIN to the canonical HTTPS origin (HTTP loopback is allowed for development)",
            ));
        }
        Ok(origin)
    }
    fn mcp_urls(&self, url: &Url) -> Result<(String, String, String)> {
        let origin = self.mcp_origin(url)?;
        let id = self.state.id();
        Ok((
            format!("{origin}/mcp/{id}/mcp"),
            format!("{origin}/oauth/{id}"),
            format!("{origin}/.well-known/oauth-protected-resource/mcp/{id}/mcp"),
        ))
    }
    pub(crate) async fn mcp_public(&self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        let (resource, issuer, metadata) = self.mcp_urls(&url)?;
        if public_workspace(url.path()) != Some(self.state.id().to_string().as_str()) {
            return api_error(404, "MCP workspace not found");
        }
        let rpc = url.path().starts_with("/mcp/");
        if rpc
            && req.headers().get("Origin")?.is_some_and(|o| {
                o != Url::parse(&resource)
                    .expect("canonical resource")
                    .origin()
                    .ascii_serialization()
            })
        {
            return api_error(403, "MCP Origin is not allowed");
        }
        if req.method() == Method::Options {
            let mut r = Response::empty()?;
            r.headers_mut().set("Access-Control-Allow-Origin", "*")?;
            r.headers_mut()
                .set("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")?;
            r.headers_mut().set(
                "Access-Control-Allow-Headers",
                "Authorization, Content-Type, Accept, MCP-Protocol-Version",
            )?;
            return Ok(r);
        }
        if !self.mcp_public_budget(&req, url.path())? {
            return api_error(429, "MCP request rate exceeded; retry in a minute");
        }
        let mut response = if url
            .path()
            .contains("/.well-known/oauth-protected-resource/")
            && req.method() == Method::Get
        {
            json(
                &json!({"resource":resource,"authorization_servers":[issuer],"scopes_supported":SCOPES,"bearer_methods_supported":["header"],"resource_name":"Kasuri workspace management"}),
                200,
            )?
        } else if url
            .path()
            .contains("/.well-known/oauth-authorization-server/")
            && req.method() == Method::Get
        {
            json(
                &json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"registration_endpoint":format!("{issuer}/register"),"revocation_endpoint":format!("{issuer}/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"revocation_endpoint_auth_methods_supported":["none"],"response_modes_supported":["query"],"code_challenge_methods_supported":["S256"],"scopes_supported":SCOPES}),
                200,
            )?
        } else {
            let content_type = req.headers().get("Content-Type")?.unwrap_or_default();
            let input = if req.method() == Method::Get {
                Value::Object(
                    url.query_pairs()
                        .map(|(k, v)| (k.into_owned(), json!(v)))
                        .collect(),
                )
            } else {
                let Some(bytes) = body(&mut req).await? else {
                    return api_error(413, "Request exceeds 128 KiB");
                };
                if content_type.split(';').next() == Some("application/json") {
                    match serde_json::from_slice(&bytes) {
                        Ok(v) => v,
                        Err(_) => {
                            return json(
                                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}}),
                                400,
                            );
                        }
                    }
                } else if !rpc
                    && content_type.split(';').next() == Some("application/x-www-form-urlencoded")
                {
                    form(&bytes)
                } else if rpc && req.method() != Method::Post {
                    Value::Null
                } else {
                    return api_error(415, "Unsupported content type");
                }
            };
            // All request body awaits precede the single synchronous load/modify/save.
            let mut ws = self.load()?;
            let mut auth: Auth = if ws.management.is_null() {
                Auth::default()
            } else {
                serde_json::from_value(ws.management.clone()).map_err(internal)?
            };
            let namespace = self.env.durable_object("WORKSPACES")?;
            let mut entropy = || namespace.unique_id().expect("secure entropy").to_string();
            let time = now();
            let nonce = entropy();
            let result = if rpc {
                let bearer = req.headers().get("Authorization")?.unwrap_or_default();
                let token = bearer.strip_prefix("Bearer ").unwrap_or("");
                let credential = match auth.authenticate(token, &resource, time) {
                    Ok(c) => c,
                    Err(code) => {
                        let mut r = json(
                            &json!({"error":code}),
                            if code == "rate_limited" { 429 } else { 401 },
                        )?;
                        r.headers_mut().set(
                            "WWW-Authenticate",
                            &format!(
                                "Bearer resource_metadata=\"{metadata}\", error=\"invalid_token\""
                            ),
                        )?;
                        return Ok(r);
                    }
                };
                if req.method() != Method::Post {
                    let mut r = api_error(
                        405,
                        "Stateless MCP accepts POST; no server-initiated SSE stream",
                    )?;
                    r.headers_mut().set("Allow", "POST, OPTIONS")?;
                    r
                } else if req.headers().get("Accept")?.is_none_or(|v| {
                    !v.contains("application/json") || !v.contains("text/event-stream")
                }) {
                    api_error(
                        406,
                        "Accept must include application/json and text/event-stream",
                    )?
                } else if req.headers().get("MCP-Protocol-Version")?.is_some_and(|v| {
                    ![workflow_mcp::PROTOCOL, "2025-06-18", "2025-03-26"].contains(&v.as_str())
                }) {
                    api_error(400, "Unsupported MCP protocol version")?
                } else {
                    match workflow_mcp::rpc(&mut ws, &mut auth, &credential, input, time, &nonce) {
                        Some(v) => json(&v, 200)?,
                        None => Response::empty()?.with_status(202),
                    }
                }
            } else if url.path().ends_with("/register") && req.method() == Method::Post {
                match auth.register(&input, time, &mut entropy) {
                    Ok(v) => json(&v, 201)?,
                    Err(e) => oauth_error(e)?,
                }
            } else if url.path().ends_with("/authorize") && req.method() == Method::Get {
                match auth.authorize(&input, &resource, time, &mut entropy) {
                    Ok(id) => {
                        let origin = self.mcp_origin(&url)?;
                        let workspace = self.browser_workspace_key()?;
                        Response::redirect(Url::parse(&format!(
                            "{origin}/w/{workspace}/mcp?authorize={id}"
                        ))?)?
                    }
                    Err(e) => oauth_error(e)?,
                }
            } else if url.path().ends_with("/token") && req.method() == Method::Post {
                match auth.exchange(&input, &resource, time, &mut entropy) {
                    Ok(v) => {
                        let digest = hash(v["access_token"].as_str().unwrap_or(""));
                        let target = auth
                            .credentials
                            .iter()
                            .find(|c| c.token_hash == digest)
                            .map(|c| c.id.clone())
                            .unwrap_or_default();
                        ws.record_audit(
                            "oauth",
                            "oauth.token_issued",
                            &target,
                            "succeeded",
                            time,
                            &nonce,
                        );
                        json(&v, 200)?
                    }
                    Err(e) => oauth_error(e)?,
                }
            } else if url.path().ends_with("/revoke") && req.method() == Method::Post {
                let digest = hash(input["token"].as_str().unwrap_or(""));
                for c in &mut auth.credentials {
                    if c.token_hash == digest || c.refresh_hash.as_ref() == Some(&digest) {
                        c.revoked = true;
                    }
                }
                json(&json!({}), 200)?
            } else {
                api_error(405, "Method not allowed")?
            };
            ws.management = serde_json::to_value(auth).map_err(internal)?;
            if ws.management.to_string().len() > 524288 {
                return api_error(429, "Management storage budget exceeded");
            }
            self.save(&mut ws).await?;
            result
        };
        let headers = response.headers().clone();
        response = response.with_headers(headers);
        response.headers_mut().set("Cache-Control", "no-store")?;
        if !rpc {
            response
                .headers_mut()
                .set("Access-Control-Allow-Origin", "*")?;
        }
        Ok(response)
    }
    pub(crate) async fn mcp_management(&self, mut req: Request) -> Result<Response> {
        let url = req.url()?;
        let (resource, issuer, _) = self.mcp_urls(&url)?;
        let method = req.method();
        let path = url.path();
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        let input = if matches!(method, Method::Get | Method::Head | Method::Delete) {
            json!({})
        } else {
            let Some(bytes) = body(&mut req).await? else {
                return api_error(413, "Request exceeds 128 KiB");
            };
            match serde_json::from_slice::<Value>(&bytes) {
                Ok(v) if v.is_object() => v,
                _ => return api_error(400, "Expected JSON object"),
            }
        };
        let mut ws = self.load()?;
        let mut auth: Auth = if ws.management.is_null() {
            Auth::default()
        } else {
            serde_json::from_value(ws.management.clone()).map_err(internal)?
        };
        let namespace = self.env.durable_object("WORKSPACES")?;
        let mut entropy = || namespace.unique_id().expect("secure entropy").to_string();
        let time = now();
        auth.prune(time);
        let result: std::result::Result<Value, &str> = match parts.as_slice() {
            ["api", "mcp"] if method == Method::Get => Ok(
                json!({"endpoint":resource,"issuer":issuer,"protocolVersion":workflow_mcp::PROTOCOL,"scopes":SCOPES,"credentials":auth.credentials.iter().map(|c|{let mut v=c.public();v["active"]=json!(!c.revoked&&c.expires_at>time);v["refreshAvailable"]=json!(!c.revoked&&c.refresh_until>time);v}).collect::<Vec<_>>(),"unusedClients":auth.clients.iter().filter(|c|!auth.credentials.iter().any(|g|g.client_id.as_deref()==Some(&c.id))).count(),"roles":{"reader":workflow_mcp::role("reader"),"builder":workflow_mcp::role("builder"),"operator":workflow_mcp::role("operator"),"manager":workflow_mcp::role("manager")}}),
            ),
            ["api", "mcp", "credentials"] if method == Method::Post => {
                let scopes: Vec<String> = input["scopes"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        workflow_mcp::role(input["role"].as_str().unwrap_or("reader"))
                    });
                let days = input["expiresDays"].as_u64().unwrap_or(30);
                if days > 90 || days == 0 {
                    Err("invalid_expiration")
                } else {
                    auth.issue(
                        input["name"].as_str().unwrap_or(""),
                        scopes,
                        &resource,
                        time,
                        days * 86400000,
                        &mut entropy,
                    )
                }
            }
            ["api", "mcp", "credentials", id] if method == Method::Delete => {
                match auth.credentials.iter_mut().find(|c| c.id == *id) {
                    Some(c) => {
                        c.revoked = true;
                        Ok(c.public())
                    }
                    None => Err("credential_not_found"),
                }
            }
            ["api", "mcp", "oauth", "clients", "reset"] if method == Method::Post => {
                auth.clients.retain(|c| {
                    auth.credentials
                        .iter()
                        .any(|g| g.client_id.as_deref() == Some(&c.id))
                });
                auth.requests
                    .retain(|r| auth.clients.iter().any(|c| c.id == r.client_id));
                Ok(json!({"cleared":true}))
            }
            ["api", "mcp", "oauth", id] if method == Method::Get => auth.consent_info(id, time),
            ["api", "mcp", "oauth", id, "approve"] if method == Method::Post => {
                let scopes = input["scopes"]
                    .as_array()
                    .map(|v| {
                        v.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                auth.consent(id, scopes, time, &mut entropy)
                    .map(|url| json!({"redirect":url}))
            }
            ["api", "mcp", "oauth", id, "deny"] if method == Method::Post => {
                match auth
                    .requests
                    .iter_mut()
                    .find(|r| r.id == *id && r.expires > time && !r.used && r.approved.is_none())
                {
                    Some(r) => {
                        r.used = true;
                        let mut redirect = Url::parse(&r.redirect).map_err(internal)?;
                        redirect
                            .query_pairs_mut()
                            .append_pair("error", "access_denied")
                            .append_pair("state", &r.state);
                        Ok(json!({"redirect":redirect.to_string()}))
                    }
                    None => Err("invalid_request"),
                }
            }
            _ => Err("route_not_found"),
        };
        if method != Method::Get {
            let target = result
                .as_ref()
                .ok()
                .and_then(|v| v["id"].as_str())
                .unwrap_or(parts.get(3).copied().unwrap_or(""));
            ws.record_audit(
                "browser",
                if path.contains("/oauth/") {
                    "oauth.consent"
                } else {
                    "mcp.credential"
                },
                target,
                if result.is_ok() {
                    "succeeded"
                } else {
                    "rejected"
                },
                time,
                "",
            );
        }
        ws.management = serde_json::to_value(auth).map_err(internal)?;
        self.save(&mut ws).await?;
        match result {
            Ok(value) => json(&value, 200),
            Err(error) => api_error(400, error),
        }
    }
}
