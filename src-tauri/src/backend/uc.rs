//! UC 网盘解析服务：分享链接 → 会话（ctoken/stoken/cookie）→ 文件列表 → 下载直链。
//! 使用 reqwest 保持现有请求参数、请求头和错误分类。

use std::time::Duration;

use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, ORIGIN, RANGE, REFERER, SET_COOKIE,
    USER_AGENT,
};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use url::Url;

use super::models::UcFileDto;
use super::util::encode_uri_component;

pub const UC_API: &str = "https://pc-api.uc.cn/1/clouddrive";
pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) uc-cloud-drive/2.5.20 Chrome/100.0.4896.160 Electron/18.3.5.4-b478491100 Safari/537.36 Channel/pckk_other_ch";
const PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct UcSession {
    pub stoken: String,
    pub ctoken: String,
    pub cookies: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UcParseResult {
    pub platform: String,
    #[serde(rename = "shareId")]
    pub share_id: String,
    #[serde(rename = "pdirFid")]
    pub pdir_fid: Option<String>,
    pub files: Vec<UcFileDto>,
    pub session: UcSession,
    #[serde(rename = "shareLink")]
    pub share_link: String,
}

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub ok: bool,
    pub kind: &'static str,
    pub detail: Option<String>,
}

fn headers_of(cookies: &str, ctoken: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(UA));
    headers.insert(REFERER, HeaderValue::from_static("https://drive.uc.cn/"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://drive.uc.cn"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    if let Ok(value) = HeaderValue::from_str(cookies) {
        headers.insert(COOKIE, value);
    }
    if !ctoken.is_empty() {
        if let Ok(value) = HeaderValue::from_str(ctoken) {
            headers.insert("x-csrf-token", value);
        }
    }
    headers
}

fn json_preview(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect()
}

async fn fetch_json(
    client: &Client,
    method: Method,
    url: &str,
    data: Option<Value>,
    cookies: &str,
    ctoken: &str,
    timeout: Duration,
) -> Result<(StatusCode, Value), String> {
    let has_data = data.is_some();
    let mut headers = headers_of(cookies, ctoken);
    if has_data {
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json;charset=UTF-8"),
        );
    }
    let mut request = client
        .request(method, url)
        .headers(headers)
        .timeout(timeout);
    if let Some(body) = data {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "_raw": text }));
    Ok((status, value))
}

/// 提取分享链接中的 shareId 与（可选的）目录 fid
pub fn extract_ids(share_link: &str) -> (Option<String>, Option<String>) {
    let share_id = share_link.find("/s/").and_then(|start| {
        let rest = &share_link[start + 3..];
        let end = rest.find(['?', '#']).unwrap_or(rest.len());
        let id = &rest[..end];
        if id.is_empty() {
            None
        } else {
            Some(id.to_string())
        }
    });
    let pdir_fid = regex::Regex::new(r"#/list/share/[^/]*/([a-f0-9]+)-")
        .ok()
        .and_then(|re| re.captures(share_link))
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()));
    (share_id, pdir_fid)
}

fn set_cookie_parts(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok().map(|s| s.to_string()))
        .collect()
}

fn merge_set_cookies(cookies: &mut String, ctoken: &mut String, set_cookies: &[String]) {
    for cookie in set_cookies {
        let parts = cookie.split(';').next().unwrap_or("");
        let Some((name, value)) = parts.split_once('=') else {
            continue;
        };
        if name == "ctoken" {
            *ctoken = value.to_string();
        }
        if !name.is_empty() && !value.is_empty() && !cookies.contains(&format!("{name}=")) {
            if !cookies.is_empty() {
                cookies.push_str("; ");
            }
            cookies.push_str(parts);
        }
    }
}

/// 访问分享页拿 ctoken，并合并 set-cookie 进会话。
/// 手动处理重定向以免丢失中间响应的 Cookie；与现有 follow 语义一致。
pub async fn get_ctoken(share_link: &str, cookie: &str) -> Result<UcSession, String> {
    let first_url = share_link.split('#').next().unwrap_or(share_link);
    let redirect_client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let mut current = Url::parse(first_url).map_err(|e| e.to_string())?;
    let mut cookies = cookie.to_string();
    let mut ctoken = String::new();
    for _ in 0..=8 {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        if let Ok(value) = HeaderValue::from_str(&cookies) {
            headers.insert(COOKIE, value);
        }
        let response = redirect_client
            .get(current.clone())
            .headers(headers)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let set_cookies = set_cookie_parts(response.headers());
        merge_set_cookies(&mut cookies, &mut ctoken, &set_cookies);
        let next = if response.status().is_redirection() {
            response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(|location| current.join(location).map_err(|e| e.to_string()))
                .transpose()?
        } else {
            None
        };
        if let Some(next) = next {
            current = next;
            continue;
        }
        // 丢弃页面内容，保持连接可复用。
        let _ = response.bytes().await;
        return Ok(UcSession {
            stoken: String::new(),
            ctoken,
            cookies,
        });
    }
    Err("分享页重定向次数过多".to_string())
}

/// 拿 stoken（分享会话令牌）
pub async fn get_stoken(
    client: &Client,
    share_id: &str,
    ctoken: &str,
    cookies: &str,
) -> Result<String, String> {
    let body = json!({
        "pwd_id": share_id,
        "passcode": "",
        "force": 0,
        "page": 1,
        "size": PAGE_SIZE,
        "fetch_banner": 1,
        "fetch_share": 1,
        "fetch_total": 1,
        "sort": "file_type:asc,file_name:asc",
        "banner_platform": "other",
        "web_platform": "windows",
        "fetch_error_background": 1,
    });
    let (_, value) = fetch_json(
        client,
        Method::POST,
        &format!("{UC_API}/share/sharepage/v2/detail?pr=UCBrowser&fr=pc"),
        Some(body),
        cookies,
        ctoken,
        Duration::from_secs(15),
    )
    .await?;
    value
        .get("data")
        .and_then(|v| v.get("token_info"))
        .and_then(|v| v.get("stoken"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("获取 stoken 失败: {}", json_preview(&value)))
}

/// 拉取目录列表（自动翻页直到拉完）
pub async fn get_file_list(
    client: &Client,
    share_id: &str,
    stoken: &str,
    pdir_fid: Option<&str>,
    ctoken: &str,
    cookies: &str,
) -> Result<Vec<Value>, String> {
    let mut all = Vec::new();
    let mut page = 1usize;
    loop {
        let mut url = format!(
            "{UC_API}/share/sharepage/detail?pr=UCBrowser&fr=pc&pwd_id={}&stoken={}&force=0&_page={page}&_size={PAGE_SIZE}&_fetch_banner=0&_fetch_share=0&_fetch_total=1&_sort=file_type:asc,file_name:asc",
            encode_uri_component(share_id),
            encode_uri_component(stoken),
        );
        if let Some(fid) = pdir_fid {
            url.push_str("&pdir_fid=");
            url.push_str(&encode_uri_component(fid));
        }
        let (_, value) = fetch_json(
            client,
            Method::GET,
            &url,
            None,
            cookies,
            ctoken,
            Duration::from_secs(15),
        )
        .await?;
        let Some(list) = value
            .get("data")
            .and_then(|v| v.get("list"))
            .and_then(Value::as_array)
        else {
            if all.is_empty() {
                return Err(format!("获取文件列表失败: {}", json_preview(&value)));
            }
            break;
        };
        all.extend(list.iter().cloned());
        if list.len() < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    Ok(all)
}

fn normalize_item(item: &Value) -> UcFileDto {
    let size = item
        .get("size")
        .and_then(Value::as_i64)
        .or_else(|| item.get("size").and_then(Value::as_f64).map(|n| n as i64))
        .unwrap_or(0);
    UcFileDto {
        fid: item
            .get("fid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: item
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        size,
        file: item.get("file").and_then(Value::as_bool).unwrap_or(false),
        format_type: item
            .get("format_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        share_fid_token: item
            .get("share_fid_token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

/// 递归展开全部文件（下载用）
pub async fn find_files(
    client: &Client,
    share_id: &str,
    stoken: &str,
    ctoken: &str,
    cookies: &str,
    pdir_fid: Option<&str>,
) -> Result<Vec<UcFileDto>, String> {
    let mut files = Vec::new();
    let mut pending = vec![pdir_fid.map(str::to_string)];
    while let Some(directory) = pending.pop() {
        let items = get_file_list(
            client,
            share_id,
            stoken,
            directory.as_deref(),
            ctoken,
            cookies,
        )
        .await?;
        for item in items {
            if item.get("file").and_then(Value::as_bool).unwrap_or(false) {
                files.push(normalize_item(&item));
            } else if let Some(fid) = item.get("fid").and_then(Value::as_str) {
                pending.push(Some(fid.to_string()));
            }
        }
    }
    Ok(files)
}

/// 单文件真实下载直链
pub async fn get_download_url(
    client: &Client,
    share_id: &str,
    stoken: &str,
    fid: &str,
    share_fid_token: &str,
    ctoken: &str,
    cookies: &str,
) -> Result<String, String> {
    let body = json!({
        "fids": [fid],
        "fids_token": [share_fid_token],
        "pwd_id": share_id,
        "stoken": stoken,
    });
    let (_, value) = fetch_json(
        client,
        Method::POST,
        &format!("{UC_API}/file/download?entry=ft&fr=pc&pr=UCBrowser"),
        Some(body),
        cookies,
        ctoken,
        Duration::from_secs(15),
    )
    .await?;
    value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|data| data.first())
        .and_then(|item| item.get("download_url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("获取下载链接失败: {}", json_preview(&value)))
}

/// 预检下载直链，分类为 ok/cookie_expired/url_invalid/http/network。
pub async fn probe_download_url(
    client: &Client,
    url: &str,
    cookies: &str,
    timeout: Duration,
) -> ProbeResult {
    let mut headers = headers_of(cookies, "");
    headers.insert(RANGE, HeaderValue::from_static("bytes=0-4095"));
    let response = match client
        .get(url)
        .headers(headers)
        .timeout(timeout)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return ProbeResult {
                ok: false,
                kind: "network",
                detail: None,
            };
        }
    };
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if status == StatusCode::OK || status == StatusCode::PARTIAL_CONTENT {
        return ProbeResult {
            ok: true,
            kind: "ok",
            detail: None,
        };
    }
    let low = text.to_lowercase();
    if status == StatusCode::FORBIDDEN
        && (low.contains("require login")
            || low.contains("auth expired")
            || low.contains("auth not found")
            || low.contains("not logged")
            || low.contains("login required"))
    {
        return ProbeResult {
            ok: false,
            kind: "cookie_expired",
            detail: Some(text.chars().take(120).collect()),
        };
    }
    if status == StatusCode::FORBIDDEN
        && (low.contains("signaturedoesnotmatch")
            || low.contains("accessdenied")
            || low.contains("request has expired")
            || low.contains("expired"))
    {
        return ProbeResult {
            ok: false,
            kind: "url_invalid",
            detail: Some(text.chars().take(120).collect()),
        };
    }
    ProbeResult {
        ok: false,
        kind: "http",
        detail: Some(format!("HTTP {}", status.as_u16())),
    }
}

/// 完整解析：分享链接 → 会话 + 当前目录文件列表
pub async fn parse(
    client: &Client,
    share_link: &str,
    cookie: &str,
) -> Result<UcParseResult, String> {
    let (share_id, pdir_fid) = extract_ids(share_link);
    let Some(share_id) = share_id else {
        return Err("无法提取 share_id，请检查链接格式".to_string());
    };
    let mut session = get_ctoken(share_link, cookie).await?;
    session.stoken = get_stoken(client, &share_id, &session.ctoken, &session.cookies).await?;
    let items = get_file_list(
        client,
        &share_id,
        &session.stoken,
        pdir_fid.as_deref(),
        &session.ctoken,
        &session.cookies,
    )
    .await?;
    if items.is_empty() {
        return Err("未找到文件".to_string());
    }
    Ok(UcParseResult {
        platform: "uc".to_string(),
        share_id,
        pdir_fid,
        files: items.iter().map(normalize_item).collect(),
        session,
        share_link: share_link.to_string(),
    })
}

/// 目录浏览（保留会话）
pub async fn list_folder(
    client: &Client,
    share_id: &str,
    stoken: &str,
    pdir_fid: Option<&str>,
    ctoken: &str,
    cookies: &str,
) -> Result<Vec<UcFileDto>, String> {
    Ok(
        get_file_list(client, share_id, stoken, pdir_fid, ctoken, cookies)
            .await?
            .iter()
            .map(normalize_item)
            .collect(),
    )
}

/// 下载直链（保留会话），供创建任务前调用
pub async fn resolve_download(
    client: &Client,
    share_id: &str,
    stoken: &str,
    fid: &str,
    share_fid_token: &str,
    ctoken: &str,
    cookies: &str,
) -> Result<String, String> {
    get_download_url(
        client,
        share_id,
        stoken,
        fid,
        share_fid_token,
        ctoken,
        cookies,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 分享链接解析() {
        let (share, dir) = extract_ids("https://drive.uc.cn/s/abc123xyz?public=1#/list/share/x/9f86d081884c7d659a2feaa0c55ad015-8e0/-1");
        assert_eq!(share.as_deref(), Some("abc123xyz"));
        assert_eq!(dir.as_deref(), Some("9f86d081884c7d659a2feaa0c55ad015"));
        assert_eq!(extract_ids("https://example.com/not-uc").0, None);
    }
}
