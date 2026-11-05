//! 由 scripts/test-cloud.mjs 提供独立 HTTPS 夹具；真实引擎与数据均隔离到临时目录。
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde_json::{json, Value};
use uc_drive2_lib::backend::{self, cloud, crypto, routes, AppState, StartOptions};

struct DenyExternalDns;
impl reqwest::dns::Resolve for DenyExternalDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let message = format!("测试禁止访问外部 DNS: {}", name.as_str());
        Box::pin(async move { Err(std::io::Error::other(message).into()) })
    }
}

fn official_client() -> Client {
    let cert = std::fs::read(std::env::var("UCDRIVE2_TEST_CA").expect("请运行 npm run test:cloud"))
        .unwrap();
    let addr: SocketAddr = format!(
        "127.0.0.1:{}",
        std::env::var("UCDRIVE2_TEST_HTTPS_PORT").unwrap()
    )
    .parse()
    .unwrap();
    let mut builder = Client::builder()
        .no_proxy()
        .add_root_certificate(reqwest::Certificate::from_pem(&cert).unwrap())
        .dns_resolver(Arc::new(DenyExternalDns));
    for host in [
        "drive-h.quark.cn",
        "drive-pc.quark.cn",
        "pan.baidu.com",
        "api-pan.xunlei.com",
    ] {
        builder = builder.resolve(host, addr);
    }
    builder.build().unwrap()
}

fn credentials(scenario: &str) -> Value {
    json!({
        "cookie": format!("auth=test-secret; scenario={scenario}"),
        "authorization": "test-secret-xl", "captchaToken": "test-captcha",
        "deviceId": "test-device", "clientVersion": "1.92.89"
    })
}

fn link(platform: &str) -> &'static str {
    match platform {
        "quark" => "https://pan.quark.cn/s/qMock?pwd=a1b2",
        "baidu" => "https://pan.baidu.com/s/1bdMock?pwd=a1b2",
        "xunlei" => "https://pan.xunlei.com/s/xlMock?pwd=a1b2",
        _ => unreachable!(),
    }
}

fn drive_file(value: &Value) -> cloud::DriveFile {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn official_links_preserve_password_and_reject_foreign_hosts() {
    let (platform, id, pass, canonical) =
        cloud::parse_link("链接：http://yun.baidu.com/s/1bdMock 提取码：a1b2").unwrap();
    assert_eq!(
        (platform.as_str(), id.as_str(), pass.as_str()),
        ("baidu", "1bdMock", "a1b2")
    );
    assert_eq!(canonical, link("baidu"));
    assert_eq!(
        cloud::parse_link("https://pan.baidu.com/share/init?surl=bdMock&pwd=a1b2")
            .unwrap()
            .1,
        "1bdMock"
    );
    assert_eq!(
        cloud::parse_link("https://pan.xunlei.com/s/xlMock#a1b2")
            .unwrap()
            .2,
        "a1b2"
    );
    for invalid in [
        "https://evil.example/s/id",
        "https://pan.baidu.com.evil.example/s/id",
        "https://pan.baidu.com@evil.example/s/id",
        "file:///s/id",
        "https://pan.quark.cn/",
    ] {
        assert!(
            cloud::parse_link(invalid).is_err(),
            "错误接受链接: {invalid}"
        );
    }
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn quark_paginates_without_total_and_lists_children() {
    let client = official_client();
    let parsed = cloud::parse(&client, link("quark"), &credentials("quark-pages"))
        .await
        .unwrap();
    assert_eq!(parsed["files"].as_array().unwrap().len(), 51);
    assert_eq!(parsed["files"][50]["fid"], "q-last");
    let mut session = parsed["session"].clone();
    let files = cloud::list_folder(
        &client,
        "quark",
        "qMock",
        Some("q-folder"),
        &mut session,
        &credentials(""),
    )
    .await
    .unwrap();
    assert_eq!(files[0].parent_id, "q-folder");
    assert_eq!(files[0].fid, "q-child");
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn baidu_reads_short_pages_and_official_share_session() {
    let client = official_client();
    let parsed = cloud::parse(&client, link("baidu"), &credentials(""))
        .await
        .unwrap();
    assert_eq!(parsed["files"].as_array().unwrap().len(), 3);
    assert_eq!(parsed["session"]["uk"], "10086");
    assert_eq!(parsed["session"]["shareid"], "20002");
    assert_eq!(parsed["session"]["sekey"], "decoded/share==");
    assert_eq!(parsed["files"][1]["fid"], "9007199254740993");
    let file = drive_file(&parsed["files"][1]);
    let resolved = cloud::resolve(
        &client,
        "baidu",
        "1bdMock",
        &parsed["session"],
        &file,
        &credentials(""),
    )
    .await
    .unwrap();
    assert!(resolved.url.ends_with("/baidu/9007199254740993"));
    let error = cloud::parse(
        &client,
        "https://pan.baidu.com/s/1bdMock?pwd=bad1",
        &credentials(""),
    )
    .await
    .unwrap_err();
    assert!(error.message.contains("提取码"));
    assert_ne!(
        error.kind.as_deref(),
        Some("credential_expired"),
        "错误提取码不能误报 Cookie 失效"
    );
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn xunlei_paginates_and_resolves_direct_files() {
    let client = official_client();
    let parsed = cloud::parse(&client, link("xunlei"), &credentials(""))
        .await
        .unwrap();
    assert_eq!(parsed["files"].as_array().unwrap().len(), 3);
    let file = drive_file(&parsed["files"][1]);
    let resolved = cloud::resolve(
        &client,
        "xunlei",
        "xlMock",
        &parsed["session"],
        &file,
        &credentials(""),
    )
    .await
    .unwrap();
    assert!(resolved.url.ends_with("/xunlei/xl-direct"));
    assert!(resolved.headers.get("Authorization").is_none());
    assert!(cloud::resolve(
        &client,
        "xunlei",
        "xlMock",
        &parsed["session"],
        &file,
        &credentials("xl-wrong-file")
    )
    .await
    .is_err());
    let mut session = parsed["session"].clone();
    let files = cloud::list_folder(
        &client,
        "xunlei",
        "xlMock",
        Some("xl-folder"),
        &mut session,
        &credentials(""),
    )
    .await
    .unwrap();
    assert_eq!(files[0].parent_id, "xl-folder");
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn upstream_expiry_and_http_errors_are_classified() {
    let client = official_client();
    for platform in ["quark", "baidu", "xunlei"] {
        for scenario in ["expired", "http403"] {
            let err = cloud::parse(&client, link(platform), &credentials(scenario))
                .await
                .unwrap_err();
            assert_eq!(
                err.kind.as_deref(),
                Some("credential_expired"),
                "{platform} {scenario}: {err:?}"
            );
        }
    }
    let err = cloud::parse(&client, link("quark"), &json!({}))
        .await
        .unwrap_err();
    assert!(err.message.contains("cookies"));
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn wrong_file_and_non_http_downloads_are_rejected() {
    let client = official_client();
    for scenario in ["wrongfid", "badurl"] {
        let credential = credentials(scenario);
        let parsed = cloud::parse(&client, link("quark"), &credential)
            .await
            .unwrap();
        let file = drive_file(&parsed["files"][1]);
        assert!(cloud::resolve(
            &client,
            "quark",
            "qMock",
            &parsed["session"],
            &file,
            &credential
        )
        .await
        .is_err());
    }
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn xunlei_restores_only_selected_file_after_completion() {
    let client = official_client();
    let credential = credentials("restore-ok");
    let parsed = cloud::parse(&client, link("xunlei"), &credential)
        .await
        .unwrap();
    let file = drive_file(&parsed["files"][2]);
    let result = cloud::resolve(
        &client,
        "xunlei",
        "xlMock",
        &parsed["session"],
        &file,
        &credential,
    )
    .await
    .unwrap();
    assert!(result.url.ends_with("/xunlei/xl-restore"));
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn xunlei_rejects_failed_restore_and_wrong_file_mapping() {
    let client = official_client();
    for scenario in ["restore-error", "wrong-map"] {
        let credential = credentials(scenario);
        let parsed = cloud::parse(&client, link("xunlei"), &credential)
            .await
            .unwrap();
        let file = drive_file(&parsed["files"][2]);
        let err = cloud::resolve(
            &client,
            "xunlei",
            "xlMock",
            &parsed["session"],
            &file,
            &credential,
        )
        .await
        .err()
        .unwrap();
        assert!(err.message.contains("转存"));
    }
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn repeated_pagination_token_is_an_error() {
    let err = cloud::parse(
        &official_client(),
        link("xunlei"),
        &credentials("repeat-token"),
    )
    .await
    .unwrap_err();
    assert!(err.message.contains("分页令牌重复"));
}

struct TestApp {
    state: Arc<AppState>,
    base: String,
    http: Client,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for TestApp {
    fn drop(&mut self) {
        self.server.abort();
        self.state.gopeed.before_update_exit();
    }
}
impl TestApp {
    async fn new(engine: bool) -> Self {
        let data = PathBuf::from(std::env::var("UCDRIVE2_TEST_ROOT").unwrap())
            .join(format!("app-{}", rand::random::<u64>()));
        let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bin/gopeed/gopeed-web.exe");
        let options = StartOptions::new(data, None, exe);
        let mut state = match Arc::try_unwrap(backend::build_state(&options).unwrap()) {
            Ok(state) => state,
            Err(_) => panic!("状态应只有一个所有者"),
        };
        let client = official_client();
        Arc::get_mut(&mut state.tasks).unwrap().client = client.clone();
        state.client = client;
        let state = Arc::new(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let router = routes::build_router(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let app = Self {
            state,
            base,
            http: Client::builder().no_proxy().build().unwrap(),
            server,
        };
        if engine {
            app.state.gopeed.start().await.unwrap();
        }
        app
    }

    async fn request(&self, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .http
            .request(method, format!("{}{path}", self.base))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        (status, response.json().await.unwrap())
    }

    async fn wait_done(&self, id: i64) -> Value {
        for _ in 0..150 {
            self.state
                .tasks
                .sync_from_gopeed(self.state.gopeed.list_tasks().await.unwrap())
                .await
                .unwrap();
            let task = self.state.tasks.get(id).unwrap().unwrap();
            if task.status == "done" {
                return json!(task);
            }
            assert_ne!(task.status, "error", "下载失败: {}", task.error);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("下载超时");
    }
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn local_api_encrypts_credentials_and_blocks_external_origins() {
    let app = TestApp::new(false).await;
    let path = "/api/cloud/credentials/quark";
    assert_eq!(
        app.request(Method::PUT, path, json!({"cookie":"test-secret"}))
            .await
            .0,
        StatusCode::OK
    );
    let (status, body) = app.request(Method::GET, path, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"configured":true}));
    {
        let db = app.state.db.lock().unwrap();
        let stored = crypto::get_setting(&db, "cloud_quark", "");
        assert!(stored.starts_with("v1:"));
        assert!(!stored.contains("test-secret"));
        assert!(crypto::decrypt_secret(&app.state.paths.data_dir, &stored).contains("test-secret"));
    }
    for origin in [
        "https://evil.example",
        "null",
        "http://tauri.localhost.evil.example",
    ] {
        let response = app
            .http
            .put(format!("{}{path}", app.base))
            .header("Origin", origin)
            .json(&json!({"cookie":"attacker-value"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(response
            .headers()
            .get("Access-Control-Allow-Origin")
            .is_none());
    }
    let preflight = app
        .http
        .request(Method::OPTIONS, format!("{}{path}", app.base))
        .header("Origin", "https://evil.example")
        .header("Access-Control-Request-Method", "PUT")
        .send()
        .await
        .unwrap();
    assert_eq!(preflight.status(), StatusCode::FORBIDDEN);
    let response = app
        .http
        .get(format!("{}/api/health", app.base))
        .header("Host", "evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for origin in [
        "http://tauri.localhost",
        "http://localhost:5173",
        "tauri://localhost",
    ] {
        let response = app
            .http
            .get(format!("{}/api/health", app.base))
            .header("Origin", origin)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["access-control-allow-origin"], origin);
    }
    assert_eq!(
        app.request(Method::PUT, path, json!({"cookie":"bad\r\nvalue"}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.request(Method::DELETE, path, Value::Null).await.0,
        StatusCode::OK
    );
    assert_eq!(
        app.request(Method::GET, path, Value::Null).await.1["configured"],
        false
    );
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具"]
async fn mismatched_share_and_folder_download_are_rejected() {
    let app = TestApp::new(false).await;
    let body = json!({"platform":"quark","shareId":"qMock","shareLink":link("baidu"),
        "session":{},"file":{"fid":"f","name":"f.bin","size":1,"file":true}});
    assert_eq!(
        app.request(Method::POST, "/api/cloud/download", body)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.request(
            Method::POST,
            "/api/cloud/list-folder",
            json!({"platform":"quark","shareId":"qMock","session":"bad"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
#[ignore = "npm run test:cloud 提供本地 HTTPS 夹具和真实 Gopeed"]
async fn three_platforms_download_pause_register_and_redownload_with_real_gopeed() {
    let app = TestApp::new(true).await;
    let result = app
        .request(
            Method::PUT,
            "/api/tasks/config",
            json!({"httpConnections":1,"maxRunning":2}),
        )
        .await;
    assert_eq!(result.0, StatusCode::OK, "{result:?}");
    for platform in ["quark", "baidu", "xunlei"] {
        let save = app
            .request(
                Method::PUT,
                &format!("/api/cloud/credentials/{platform}"),
                credentials("integration"),
            )
            .await;
        assert_eq!(save.0, StatusCode::OK, "{save:?}");
        let parsed = app
            .request(
                Method::POST,
                "/api/cloud/parse",
                json!({"shareLink":link(platform)}),
            )
            .await;
        assert_eq!(parsed.0, StatusCode::OK, "{parsed:?}");
        let mut share = parsed.1;
        let folder = share["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["file"] == false)
            .unwrap();
        let listing = app.request(Method::POST, "/api/cloud/list-folder", json!({
            "platform":platform,"shareId":share["shareId"],"session":share["session"],"pdirFid":folder["fid"]
        })).await;
        assert_eq!(listing.0, StatusCode::OK, "{listing:?}");
        share["session"] = listing.1["session"].clone();
        share["file"] = listing.1["files"][0].clone();
        let created = app
            .request(Method::POST, "/api/cloud/download", share.clone())
            .await;
        assert_eq!(created.0, StatusCode::CREATED, "{created:?}");
        let id = created.1["id"].as_i64().unwrap();
        assert_eq!(created.1["source"], platform);
        assert!(!created.1["metadata"].to_string().contains("test-secret"));
        let gid = created.1["gopeed_id"].as_str().unwrap();
        let engine = app.state.gopeed.get_task(gid).await.unwrap();
        assert_eq!(
            engine["meta"]["req"]["extra"]["header"]["Referer"],
            match platform {
                "quark" => "https://pan.quark.cn/",
                "baidu" => "https://pan.baidu.com/",
                _ => "https://pan.xunlei.com/",
            }
        );
        if platform == "quark" {
            let paused = app
                .request(Method::POST, &format!("/api/tasks/{id}/pause"), Value::Null)
                .await;
            assert_eq!(paused.0, StatusCode::OK, "{paused:?}");
            assert_eq!(paused.1["status"], "paused");
            assert_eq!(
                app.request(
                    Method::POST,
                    &format!("/api/tasks/{id}/resume"),
                    Value::Null
                )
                .await
                .0,
                StatusCode::OK
            );
        }
        app.wait_done(id).await;
        let filename = created.1["metadata"]["cloud"]["filename"].as_str().unwrap();
        let bytes = std::fs::read(app.state.storage.get().join(filename)).unwrap();
        let expected: Vec<u8> = (0..524288).map(|i| (i % 251) as u8).collect();
        assert_eq!(bytes, expected, "下载文件内容必须完整一致");
        let tree = app
            .request(Method::GET, "/api/files?parent=root", Value::Null)
            .await;
        assert!(tree
            .1
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["name"] == filename));
        let again = app
            .request(
                Method::POST,
                &format!("/api/cloud/redownload/{id}"),
                Value::Null,
            )
            .await;
        assert_eq!(again.0, StatusCode::CREATED, "{again:?}");
        app.wait_done(again.1["id"].as_i64().unwrap()).await;
        assert_ne!(again.1["id"], id);
        let remove = app
            .request(
                Method::POST,
                &format!("/api/tasks/{id}/delete"),
                json!({"force":false}),
            )
            .await;
        assert_eq!(remove.0, StatusCode::OK);
    }
    let history = app.request(Method::GET, "/api/history", Value::Null).await;
    assert_eq!(history.1.as_array().unwrap().len(), 3);
}
