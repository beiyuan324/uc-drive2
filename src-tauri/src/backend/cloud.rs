//! 夸克、百度、迅雷官方分享接口；文件传输统一由任务服务交给 Gopeed。

use std::collections::HashSet;
use std::time::Duration;

use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use super::error::{ApiError, ApiResult};
use super::uc;
use super::util::encode_uri_component;

pub const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const QUARK_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/3.20.0 Chrome/112.0.5615.165 Electron/24.1.3.8 Safari/537.36 Channel/pckk_other_ch";
const XL_API: &str = "https://api-pan.xunlei.com/drive/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    pub fid: String,
    pub name: String,
    pub size: i64,
    pub file: bool,
    #[serde(default)]
    pub format_type: String,
    #[serde(default)]
    pub share_fid_token: String,
    #[serde(default)]
    pub parent_id: String,
}

pub struct ResolvedDownload {
    pub url: String,
    pub headers: Value,
}

pub fn platform_name(platform: &str) -> ApiResult<&'static str> {
    match platform {
        "quark" => Ok("夸克"),
        "baidu" => Ok("百度"),
        "xunlei" => Ok("迅雷"),
        _ => Err(ApiError::bad_request("不支持的网盘类型")),
    }
}

pub fn text(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn required(value: &Value, key: &str) -> ApiResult<String> {
    let result = text(value, key);
    if result.is_empty() {
        Err(ApiError::bad_request(format!(
            "网盘会话缺少 {key}，请重新解析"
        )))
    } else {
        Ok(result)
    }
}

fn size(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or(0)
}

async fn response(request: RequestBuilder, platform: &str) -> ApiResult<Value> {
    let name = platform_name(platform)?;
    let res = request
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("{name}接口请求失败: {e}")))?;
    let status = res.status();
    if matches!(status.as_u16(), 401 | 403) {
        return Err(ApiError::with_kind(
            403,
            "官方接口拒绝访问，请更新该网盘的登录凭据或校验令牌",
            "credential_expired",
        ));
    }
    if !status.is_success() {
        return Err(ApiError::internal(format!("官方接口返回 HTTP {status}")));
    }
    let data: Value = res
        .json()
        .await
        .map_err(|_| ApiError::internal("官方接口未返回 JSON，可能需要在官网完成登录或验证码"))?;
    let code = text(
        &data,
        match platform {
            "quark" => "code",
            "baidu" => "errno",
            _ => "error_code",
        },
    );
    if !code.is_empty() && code != "0" || data.get("error").is_some_and(|v| !v.is_null() && v != "")
    {
        let message = [
            "error_description",
            "errmsg",
            "show_msg",
            "message",
            "msg",
            "error",
        ]
        .iter()
        .find_map(|k| data[*k].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("官方接口返回业务错误");
        let message = format!("{}: {message}（{code}）", platform_name(platform)?);
        if platform == "quark" && code == "31001"
            || platform == "baidu" && code == "-6"
            || platform == "xunlei" && matches!(code.as_str(), "9" | "16")
        {
            return Err(ApiError::with_kind(403, message, "credential_expired"));
        }
        return Err(ApiError::internal(message));
    }
    Ok(data)
}

fn web_request(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    cookie: &str,
    platform: &str,
) -> RequestBuilder {
    let origin = if platform == "quark" {
        "https://pan.quark.cn"
    } else {
        "https://pan.baidu.com"
    };
    client
        .request(method, url)
        .header("User-Agent", BROWSER_UA)
        .header("Cookie", cookie)
        .header("Referer", format!("{origin}/"))
        .header("Origin", origin)
}

pub fn parse_link(input: &str) -> ApiResult<(String, String, String, String)> {
    let raw = regex::Regex::new(r#"https?://[^\s<>「」《》]+"#)
        .unwrap()
        .find(input)
        .map(|m| m.as_str().trim_end_matches(['，', '。', ')', '）']))
        .unwrap_or(input.trim());
    let url = Url::parse(raw).map_err(|_| ApiError::bad_request("分享链接格式不正确"))?;
    let host = url.host_str().unwrap_or("");
    let platform = match host {
        "pan.quark.cn" => "quark",
        "pan.baidu.com" | "yun.baidu.com" => "baidu",
        "pan.xunlei.com" => "xunlei",
        "drive.uc.cn" | "fast.uc.cn" => "uc",
        _ => {
            return Err(ApiError::bad_request(
                "支持 UC、夸克、百度和迅雷的官方分享链接",
            ))
        }
    };
    let id = url
        .path()
        .strip_prefix("/s/")
        .filter(|s| !s.is_empty())
        .map(|s| s.split('/').next().unwrap().to_string())
        .or_else(|| {
            url.query_pairs().find(|(k, _)| k == "surl").map(|(_, v)| {
                if platform == "baidu" {
                    format!("1{v}")
                } else {
                    v.into_owned()
                }
            })
        })
        .ok_or_else(|| ApiError::bad_request("链接中缺少分享 ID"))?;
    let passcode = url
        .query_pairs()
        .find(|(k, _)| matches!(k.as_ref(), "pwd" | "passcode" | "pass_code"))
        .map(|(_, v)| v.into_owned())
        .or_else(|| {
            regex::Regex::new(r"(?:提取码|访问码|密码)\s*[:：]?\s*([A-Za-z0-9]{4})")
                .unwrap()
                .captures(input)
                .map(|c| c[1].to_string())
        })
        .or_else(|| {
            url.fragment()
                .filter(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_alphanumeric()))
                .map(str::to_string)
        })
        .unwrap_or_default();
    let canonical_host = match platform {
        "baidu" => "pan.baidu.com",
        "quark" => "pan.quark.cn",
        "xunlei" => "pan.xunlei.com",
        _ => host,
    };
    let mut link = Url::parse(&format!("https://{canonical_host}/s/{id}"))
        .map_err(|_| ApiError::bad_request("分享 ID 格式不正确"))?;
    if !passcode.is_empty() {
        link.query_pairs_mut().append_pair("pwd", &passcode);
    }
    Ok((platform.to_string(), id, passcode, link.to_string()))
}

pub async fn parse(client: &Client, input: &str, credential: &Value) -> ApiResult<Value> {
    let (platform, share_id, passcode, share_link) = parse_link(input)?;
    platform_name(&platform)?;
    let mut session = json!({ "passcode": passcode, "cookies": text(credential, "cookie") });
    match platform.as_str() {
        "quark" => {
            let cookie = required(&session, "cookies")?;
            let data = response(web_request(client, reqwest::Method::POST,
                "https://drive-h.quark.cn/1/clouddrive/share/sharepage/token?pr=ucpro&fr=pc&uc_param_str=",
                &cookie, "quark").json(&json!({
                    "pwd_id": share_id, "passcode": passcode, "support_visit_limit_private_share": true
                })), "quark").await?;
            session["stoken"] = json!(required(&data["data"], "stoken")?);
        }
        "baidu" => {
            let mut cookie = required(&session, "cookies")?;
            let tpl = response(
                web_request(
                    client,
                    reqwest::Method::GET,
                    "https://pan.baidu.com/api/gettemplatevariable",
                    &cookie,
                    "baidu",
                )
                .query(&[
                    ("clienttype", "0"),
                    ("app_id", "250528"),
                    ("web", "1"),
                    ("fields", r#"["bdstoken"]"#),
                ]),
                "baidu",
            )
            .await?;
            let bdstoken = required(&tpl["result"], "bdstoken")?;
            session["bdstoken"] = json!(bdstoken);
            if !passcode.is_empty() {
                let data = response(
                    web_request(
                        client,
                        reqwest::Method::POST,
                        "https://pan.baidu.com/share/verify",
                        &cookie,
                        "baidu",
                    )
                    .query(&[
                        ("surl", share_id.strip_prefix('1').unwrap_or(&share_id)),
                        ("bdstoken", &bdstoken),
                        ("web", "1"),
                        ("clienttype", "0"),
                    ])
                    .form(&[
                        ("pwd", passcode.as_str()),
                        ("vcode", ""),
                        ("vcode_str", ""),
                    ]),
                    "baidu",
                )
                .await?;
                let randsk = required(&data, "randsk")?;
                cookie = uc::merge_cookies(&cookie, &format!("BDCLND={randsk}"));
                session["sekey"] = json!(percent_encoding::percent_decode_str(&randsk)
                    .decode_utf8_lossy()
                    .into_owned());
                session["cookies"] = json!(cookie);
            } else {
                let sekey = cookie
                    .split(';')
                    .find_map(|p| p.trim().strip_prefix("BDCLND="))
                    .map(|s| {
                        percent_encoding::percent_decode_str(s)
                            .decode_utf8_lossy()
                            .into_owned()
                    })
                    .unwrap_or_default();
                session["sekey"] = json!(sekey);
            }
        }
        "xunlei" => {}
        _ => unreachable!(),
    }
    let files = list_folder(client, &platform, &share_id, None, &mut session, credential).await?;
    Ok(json!({
        "platform": platform, "shareId": share_id, "shareLink": share_link, "pdirFid": null,
        "files": files, "session": session, "cookieUsed": !text(credential, "cookie").is_empty()
            || !text(credential, "authorization").is_empty()
    }))
}

pub async fn list_folder(
    client: &Client,
    platform: &str,
    share_id: &str,
    parent: Option<&str>,
    session: &mut Value,
    credential: &Value,
) -> ApiResult<Vec<DriveFile>> {
    platform_name(platform)?;
    let mut files = Vec::new();
    let mut page_token = String::new();
    let mut seen_tokens = HashSet::new();
    for page in 1..=100 {
        let data = match platform {
            "quark" => {
                let cookie =
                    uc::merge_cookies(&text(session, "cookies"), &text(credential, "cookie"));
                session["cookies"] = json!(cookie);
                response(
                    web_request(
                        client,
                        reqwest::Method::GET,
                        "https://drive-h.quark.cn/1/clouddrive/share/sharepage/detail",
                        &cookie,
                        platform,
                    )
                    .query(&[
                        ("pr", "ucpro"),
                        ("fr", "pc"),
                        ("ver", "2"),
                        ("pwd_id", share_id),
                        ("stoken", &required(session, "stoken")?),
                        ("pdir_fid", parent.unwrap_or("0")),
                        ("_page", &page.to_string()),
                        ("_size", "50"),
                        ("_fetch_total", "1"),
                        ("_fetch_share", "1"),
                        ("_sort", "file_type:asc,file_name:asc"),
                    ]),
                    platform,
                )
                .await?
            }
            "baidu" => {
                let data = response(web_request(client, reqwest::Method::POST,
                    "https://pan.baidu.com/share/wxlist?channel=weixin&version=2.9.6&clienttype=25&web=1",
                    &required(session, "cookies")?, platform)
                    .form(&[("shorturl", share_id), ("pwd", &text(session, "passcode")),
                        ("dir", parent.unwrap_or("/")), ("root", if parent.is_none() { "1" } else { "0" }),
                        ("page", &page.to_string()), ("num", "100"), ("order", "filename")]), platform).await?;
                for key in ["uk", "shareid", "seckey"] {
                    if !text(&data["data"], key).is_empty() {
                        session[if key == "seckey" { "sekey" } else { key }] =
                            data["data"][key].clone();
                    }
                }
                data
            }
            "xunlei" => {
                let endpoint = if parent.is_some() {
                    "share/detail"
                } else {
                    "share"
                };
                let data = response(
                    xl_request(client, reqwest::Method::GET, endpoint, credential)?.query(&[
                        ("share_id", share_id),
                        ("pass_code", &text(session, "passcode")),
                        ("pass_code_token", &text(session, "pass_code_token")),
                        ("parent_id", parent.unwrap_or("")),
                        ("page_token", page_token.as_str()),
                        ("limit", "100"),
                        ("space", ""),
                        ("scene", "NORMAL"),
                    ]),
                    platform,
                )
                .await?;
                if let Some(token) = data.get("pass_code_token").and_then(Value::as_str) {
                    session["pass_code_token"] = json!(token);
                }
                data
            }
            _ => unreachable!(),
        };
        let list = match platform {
            "quark" => data["data"]["list"].as_array(),
            "baidu" => data["data"]["list"].as_array(),
            _ => data["files"].as_array(),
        }
        .ok_or_else(|| ApiError::internal("官方接口缺少文件列表，请重新解析"))?;
        for item in list {
            let is_file = match platform {
                "quark" => item["file"].as_bool().unwrap_or(item["dir"] != true),
                "baidu" => size(&item["isdir"]) != 1,
                _ => item["kind"] != "drive#folder",
            };
            let (fid, name) = match platform {
                "quark" => (text(item, "fid"), text(item, "file_name")),
                "baidu" => (
                    if is_file {
                        text(item, "fs_id")
                    } else {
                        text(item, "path")
                    },
                    text(item, "server_filename"),
                ),
                _ => (text(item, "id"), text(item, "name")),
            };
            if fid.is_empty() || name.is_empty() {
                return Err(ApiError::internal("官方文件条目缺少 ID 或名称"));
            }
            files.push(DriveFile {
                fid,
                name,
                size: size(&item["size"]),
                file: is_file,
                format_type: text(item, "format_type"),
                share_fid_token: text(item, "share_fid_token"),
                parent_id: parent.unwrap_or("").to_string(),
            });
        }
        if platform == "xunlei" {
            let next = text(&data, "next_page_token");
            if next.is_empty() {
                return Ok(files);
            }
            if !seen_tokens.insert(next.clone()) {
                return Err(ApiError::internal("迅雷分页令牌重复，目录未读取完整"));
            }
            page_token = next;
        } else {
            if platform == "baidu" {
                if let Some(more) = data["data"].get("has_more") {
                    if !more.as_bool().unwrap_or_else(|| size(more) != 0) {
                        return Ok(files);
                    }
                    continue;
                }
            }
            let total = data
                .pointer("/metadata/_total")
                .or_else(|| data.pointer("/data/metadata/_total"))
                .map(size);
            if list.len() < if platform == "quark" { 50 } else { 100 }
                || total.is_some_and(|n| n > 0 && files.len() as i64 >= n)
            {
                return Ok(files);
            }
        }
    }
    Err(ApiError::internal(
        "目录超过 100 页，已停止读取；请缩小下载目录",
    ))
}

pub async fn resolve(
    client: &Client,
    platform: &str,
    share_id: &str,
    session: &Value,
    file: &DriveFile,
    credential: &Value,
) -> ApiResult<ResolvedDownload> {
    platform_name(platform)?;
    if !file.file {
        return Err(ApiError::bad_request("请选择文件下载"));
    }
    let (url, headers) = match platform {
        "quark" => {
            let cookie = uc::merge_cookies(&text(session, "cookies"), &text(credential, "cookie"));
            let data = response(
                web_request(
                    client,
                    reqwest::Method::POST,
                    "https://drive-pc.quark.cn/1/clouddrive/file/download?entry=ft&fr=pc&pr=ucpro",
                    &cookie,
                    platform,
                )
                .header("User-Agent", QUARK_UA)
                .json(&json!({
                    "fids": [file.fid], "fids_token": [file.share_fid_token],
                    "pwd_id": share_id, "stoken": required(session, "stoken")?
                })),
                platform,
            )
            .await?;
            let item = data["data"]
                .as_array()
                .and_then(|a| a.iter().find(|v| text(v, "fid") == file.fid))
                .ok_or_else(|| ApiError::internal("夸克未返回所选文件的下载地址"))?;
            (
                required(item, "download_url")?,
                json!({ "User-Agent": QUARK_UA, "Referer": "https://pan.quark.cn/", "Cookie": cookie }),
            )
        }
        "baidu" => {
            let cookie = required(session, "cookies")?;
            let config = response(
                web_request(
                    client,
                    reqwest::Method::GET,
                    "https://pan.baidu.com/share/tplconfig",
                    &cookie,
                    platform,
                )
                .query(&[
                    ("surl", share_id),
                    ("fields", "sign,timestamp"),
                    ("bdstoken", &required(session, "bdstoken")?),
                    ("web", "1"),
                    ("clienttype", "0"),
                    ("app_id", "250528"),
                ]),
                platform,
            )
            .await?;
            let sign = required(&config["data"], "sign")?;
            let timestamp = required(&config["data"], "timestamp")?;
            let fid = file
                .fid
                .parse::<u64>()
                .map_err(|_| ApiError::bad_request("百度文件 ID 格式错误"))?;
            let data = response(
                web_request(
                    client,
                    reqwest::Method::POST,
                    "https://pan.baidu.com/api/sharedownload",
                    &cookie,
                    platform,
                )
                .query(&[
                    ("sign", sign.as_str()),
                    ("timestamp", timestamp.as_str()),
                    ("web", "1"),
                    ("clienttype", "5"),
                    ("channel", "chunlei"),
                    ("app_id", "250528"),
                    ("bdstoken", &required(session, "bdstoken")?),
                ])
                .form(&[
                    ("encrypt", "0".to_string()),
                    ("timestamp", timestamp.clone()),
                    (
                        "extra",
                        json!({"sekey": text(session, "sekey")}).to_string(),
                    ),
                    ("fid_list", json!([fid]).to_string()),
                    ("primaryid", required(session, "shareid")?),
                    ("uk", required(session, "uk")?),
                    ("product", "share".to_string()),
                    ("type", "nolimit".to_string()),
                ]),
                platform,
            )
            .await?;
            let item = data["list"]
                .as_array()
                .and_then(|a| a.iter().find(|v| text(v, "fs_id") == file.fid))
                .ok_or_else(|| ApiError::internal("百度未返回所选文件的下载地址"))?;
            (
                required(item, "dlink")?,
                json!({ "User-Agent": "netdisk;P2SP;3.0.20.138", "Referer": "https://pan.baidu.com/", "Cookie": cookie }),
            )
        }
        "xunlei" => {
            let data = response(
                xl_request(client, reqwest::Method::GET, "share/file_info", credential)?.query(&[
                    ("share_id", share_id),
                    ("file_id", file.fid.as_str()),
                    ("pass_code", &text(session, "passcode")),
                    ("pass_code_token", &text(session, "pass_code_token")),
                    ("folder_id", file.parent_id.as_str()),
                    ("space", ""),
                ]),
                platform,
            )
            .await?;
            let info = data.get("file_info").unwrap_or(&data);
            let returned_id = text(info, "id");
            if !returned_id.is_empty() && returned_id != file.fid {
                return Err(ApiError::internal("迅雷返回的文件 ID 与所选文件不一致"));
            }
            let mut direct = text(info, "web_content_link");
            if direct.is_empty() {
                direct = xl_restore_url(client, share_id, session, file, credential).await?;
            }
            (
                direct,
                json!({ "User-Agent": BROWSER_UA, "Referer": "https://pan.xunlei.com/" }),
            )
        }
        _ => unreachable!(),
    };
    let parsed = Url::parse(&url).map_err(|_| ApiError::internal("官方接口返回了无效直链"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(ApiError::internal("官方接口返回了非 HTTP 下载地址"));
    }
    Ok(ResolvedDownload { url, headers })
}

fn xl_request(
    client: &Client,
    method: reqwest::Method,
    endpoint: &str,
    credential: &Value,
) -> ApiResult<RequestBuilder> {
    let mut request = client
        .request(method, format!("{XL_API}/{endpoint}"))
        .header("User-Agent", BROWSER_UA)
        .header("Referer", "https://pan.xunlei.com/")
        .header("Origin", "https://pan.xunlei.com")
        .header("X-Client-Id", "Xqp0kJBXWhwaTpB6");
    for (key, header) in [
        ("cookie", "Cookie"),
        ("authorization", "Authorization"),
        ("captchaToken", "X-Captcha-Token"),
        ("deviceId", "X-Device-Id"),
        ("clientVersion", "X-Client-Version"),
    ] {
        let value = text(credential, key);
        if !value.is_empty() {
            let value = if key == "authorization" && !value.starts_with("Bearer ") {
                format!("Bearer {value}")
            } else {
                value
            };
            request = request.header(header, value);
        }
    }
    Ok(request)
}

fn restore_field<'a>(data: &'a Value, field: &str) -> Option<&'a Value> {
    [
        data,
        &data["params"],
        &data["task"],
        &data["task"]["params"],
    ]
    .iter()
    .find_map(|v| v.get(field))
}

fn restored_id(data: &Value, fid: &str) -> Option<String> {
    let raw = restore_field(data, "trace_file_ids")
        .or_else(|| restore_field(data, "restore_file_ids"))?;
    let mapping: Value = if let Some(s) = raw.as_str() {
        serde_json::from_str(s).ok()?
    } else {
        raw.clone()
    };
    let value = mapping.get(fid)?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first()?.as_str().map(str::to_string),
        Value::Object(o) if o.len() == 1 => o.values().next()?.as_str().map(str::to_string),
        _ => None,
    }
}

async fn xl_restore_url(
    client: &Client,
    share_id: &str,
    session: &Value,
    file: &DriveFile,
    credential: &Value,
) -> ApiResult<String> {
    if text(credential, "authorization").is_empty() {
        return Err(ApiError::bad_request(
            "迅雷普通分享需要转存后取链，请在设置中填写官网 Authorization",
        ));
    }
    let mut data = response(xl_request(client, reqwest::Method::POST, "share/restore", credential)?
        .json(&json!({
            "parent_id": "", "share_id": share_id, "pass_code_token": required(session, "pass_code_token")?,
            "ancestor_ids": [], "file_ids": [file.fid], "specify_parent_id": true
        })), "xunlei").await?;
    let task_id = restore_field(&data, "restore_task_id")
        .or_else(|| restore_field(&data, "task_id"))
        .or_else(|| data.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let mut target = restored_id(&data, &file.fid);
    let mut complete = task_id.is_empty() && target.is_some();
    for _ in 0..60 {
        let phase = restore_field(&data, "phase")
            .or_else(|| restore_field(&data, "restore_status"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(phase, "ERROR" | "PHASE_TYPE_ERROR" | "RESTORE_ERROR") {
            return Err(ApiError::internal(
                "迅雷官方转存任务失败，请在官网检查空间和文件状态",
            ));
        }
        if matches!(
            phase,
            "COMPLETE" | "PHASE_TYPE_COMPLETE" | "RESTORE_COMPLETE"
        ) {
            if target.is_none() {
                return Err(ApiError::internal(
                    "迅雷转存结果缺少所选文件的映射，已停止取链",
                ));
            }
            complete = true;
            break;
        }
        if task_id.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        data = response(
            xl_request(
                client,
                reqwest::Method::GET,
                &format!("tasks/{}", encode_uri_component(&task_id)),
                credential,
            )?,
            "xunlei",
        )
        .await?;
        target = restored_id(&data, &file.fid).or(target);
    }
    if !complete {
        return Err(ApiError::internal(
            "迅雷转存尚未完成，请在官网检查任务后重试",
        ));
    }
    let id = target.ok_or_else(|| {
        ApiError::internal("迅雷转存未返回所选文件的 ID，请在官网检查转存任务后重试")
    })?;
    let data = response(
        xl_request(
            client,
            reqwest::Method::GET,
            &format!("files/{}", encode_uri_component(&id)),
            credential,
        )?
        .query(&[("space", ""), ("usage", "CONSUME")]),
        "xunlei",
    )
    .await?;
    if required(&data, "id")? != id {
        return Err(ApiError::internal("迅雷转存文件 ID 与下载结果不一致"));
    }
    required(&data, "web_content_link")
}
