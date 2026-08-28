use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::task::JoinHandle;

use uc_drive2_lib::backend::{self, routes};

struct TestApp {
    _dir: TempDir,
    pub base: String,
    pub client: reqwest::Client,
    pub state: Arc<backend::AppState>,
    server: JoinHandle<Result<(), std::io::Error>>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn app() -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let mut options = backend::StartOptions::new(
        dir.path().to_path_buf(),
        None,
        PathBuf::from("missing-gopeed-web.exe"),
    );
    options.base_port = 0;
    options.poll_interval = std::time::Duration::from_millis(50);
    let state = backend::build_state(&options).unwrap();
    let router: Router = routes::build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    TestApp {
        _dir: dir,
        base: format!("http://127.0.0.1:{port}"),
        client: reqwest::Client::builder().no_proxy().build().unwrap(),
        state,
        server,
    }
}

async fn json_request(
    app: &TestApp,
    method: reqwest::Method,
    path: &str,
    body: Value,
) -> reqwest::Response {
    app.client
        .request(method, format!("{}{path}", app.base))
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn upload_form(parent: &str, name: &str, bytes: &[u8]) -> Form {
    Form::new().text("parent", parent.to_string()).part(
        "files",
        Part::bytes(bytes.to_vec())
            .file_name(name.to_string())
            .mime_str("application/octet-stream")
            .unwrap(),
    )
}

async fn upload(app: &TestApp, parent: &str, name: &str, bytes: &[u8]) -> Value {
    app.client
        .post(format!("{}/api/files", app.base))
        .multipart(upload_form(parent, name, bytes))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn cors_健康检查和设置契约() {
    let app = app().await;
    let response = app
        .client
        .get(format!("{}/api/health", app.base))
        .header("Origin", "http://tauri.localhost")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["access-control-allow-origin"], "*");
    let health: Value = response.json().await.unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["gopeed"], false);

    let options = app
        .client
        .request(reqwest::Method::OPTIONS, format!("{}/api/files", app.base))
        .header("Origin", "http://tauri.localhost")
        .header("Access-Control-Request-Method", "GET")
        .header("Access-Control-Request-Headers", "content-type")
        .send()
        .await
        .unwrap();
    assert_eq!(options.status(), 204);
    assert!(options.headers()["access-control-allow-methods"]
        .to_str()
        .unwrap()
        .contains("DELETE"));
    assert!(options.headers()["access-control-allow-headers"]
        .to_str()
        .unwrap()
        .to_lowercase()
        .contains("range"));

    let settings = app
        .client
        .get(format!("{}/api/settings", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(settings.status(), 200);
    assert!(settings.headers()["access-control-expose-headers"]
        .to_str()
        .unwrap()
        .contains("Content-Range"));
    assert_eq!(
        settings.json::<Value>().await.unwrap()["storageDir"],
        app.state.storage.get().to_string_lossy().to_string()
    );
}

#[tokio::test]
async fn 目录上传下载范围和树接口() {
    let app = app().await;
    let dir = json_request(
        &app,
        reqwest::Method::POST,
        "/api/dirs",
        json!({"name":"文档","parent":"root"}),
    )
    .await;
    assert_eq!(dir.status(), 201);
    let dir: Value = dir.json().await.unwrap();
    assert_eq!(dir["is_dir"], true);
    let dir_id = dir["id"].as_i64().unwrap();

    let sub = json_request(
        &app,
        reqwest::Method::POST,
        "/api/dirs",
        json!({"name":"子目录","parent":dir_id}),
    )
    .await;
    assert_eq!(sub.status(), 201);
    let sub: Value = sub.json().await.unwrap();
    let sub_id = sub["id"].as_i64().unwrap();

    let invalid = json_request(
        &app,
        reqwest::Method::POST,
        "/api/dirs",
        json!({"name":"a/b"}),
    )
    .await;
    assert_eq!(invalid.status(), 400);

    let row = upload(
        &app,
        &sub_id.to_string(),
        "说明.txt",
        "0123456789abcdefghij".as_bytes(),
    )
    .await;
    assert_eq!(row[0]["name"], "说明.txt");
    assert_eq!(row[0]["size"], 20);
    assert_eq!(row[0]["mime"], "text/plain");
    assert_eq!(row[0]["is_dir"], false);
    let file_id = row[0]["id"].as_i64().unwrap();

    let duplicate = upload(&app, &sub_id.to_string(), "说明.txt", b"x").await;
    assert_eq!(duplicate[0]["name"], "说明 (1).txt");

    let full = app
        .client
        .get(format!("{}/api/files/{file_id}/download", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(full.status(), 200);
    assert_eq!(full.headers()["accept-ranges"], "bytes");
    assert_eq!(
        full.bytes().await.unwrap().as_ref(),
        b"0123456789abcdefghij"
    );

    let partial = app
        .client
        .get(format!("{}/api/files/{file_id}/download", app.base))
        .header("Range", "bytes=2-5")
        .send()
        .await
        .unwrap();
    assert_eq!(partial.status(), 206);
    assert_eq!(partial.headers()["content-range"], "bytes 2-5/20");
    assert_eq!(partial.bytes().await.unwrap().as_ref(), b"2345");

    let tail = app
        .client
        .get(format!("{}/api/files/{file_id}/download", app.base))
        .header("Range", "bytes=-4")
        .send()
        .await
        .unwrap();
    assert_eq!(tail.status(), 206);
    assert_eq!(tail.bytes().await.unwrap().as_ref(), b"ghij");

    let bad = app
        .client
        .get(format!("{}/api/files/{file_id}/download", app.base))
        .header("Range", "bytes=50-60")
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 416);
    assert_eq!(bad.headers()["content-range"], "bytes */20");

    let head = app
        .client
        .request(
            reqwest::Method::HEAD,
            format!("{}/api/files/{file_id}/download", app.base),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(head.status(), 200);
    assert_eq!(head.headers()["content-length"], "20");

    let renamed = json_request(
        &app,
        reqwest::Method::PATCH,
        &format!("/api/files/{file_id}"),
        json!({"name":"改名.md"}),
    )
    .await;
    assert_eq!(renamed.status(), 200);
    assert_eq!(renamed.json::<Value>().await.unwrap()["name"], "改名.md");

    let moved = json_request(
        &app,
        reqwest::Method::PATCH,
        &format!("/api/files/{file_id}"),
        json!({"parent":dir_id}),
    )
    .await;
    assert_eq!(moved.status(), 200);
    assert!(moved.json::<Value>().await.unwrap()["path"]
        .as_str()
        .unwrap()
        .contains("文档"));

    let ancestors = app
        .client
        .get(format!("{}/api/files/{sub_id}/ancestors", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(ancestors.status(), 200);
    let ancestors: Vec<Value> = ancestors.json().await.unwrap();
    assert_eq!(
        ancestors
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["文档", "子目录"]
    );

    let tree: Vec<Value> = app
        .client
        .get(format!("{}/api/tree", app.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tree[0]["name"], "文档");
    assert_eq!(tree[0]["children"][0]["name"], "子目录");

    let search: Vec<Value> = app
        .client
        .get(format!("{}/api/search?q=改名", app.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(search.iter().any(|v| v["name"] == "改名.md"));

    let cycle = json_request(
        &app,
        reqwest::Method::PATCH,
        &format!("/api/files/{dir_id}"),
        json!({"parent":sub_id}),
    )
    .await;
    assert_eq!(cycle.status(), 400);

    let dir_download = app
        .client
        .get(format!("{}/api/files/{dir_id}/download", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(dir_download.status(), 400);
}

#[tokio::test]
async fn 删除目录和临时_torrent_上传() {
    let app = app().await;
    let dir: Value = json_request(
        &app,
        reqwest::Method::POST,
        "/api/dirs",
        json!({"name":"待删除"}),
    )
    .await
    .json()
    .await
    .unwrap();
    let dir_id = dir["id"].as_i64().unwrap();
    let file = upload(&app, &dir_id.to_string(), "a.txt", b"x").await;
    let delete = app
        .client
        .delete(format!("{}/api/files/{dir_id}", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(delete.status(), 200);
    assert_eq!(delete.json::<Value>().await.unwrap()["ok"], true);
    let gone = app
        .client
        .get(format!("{}/api/files/{dir_id}", app.base))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404);
    assert!(file[0]["id"].as_i64().is_some());

    let form = Form::new().part(
        "file",
        Part::bytes(b"d8:announce0e".to_vec()).file_name("sample.torrent"),
    );
    let tmp = app
        .client
        .post(format!("{}/api/tmp-files", app.base))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(tmp.status(), 201);
    let tmp: Value = tmp.json().await.unwrap();
    assert!(tmp["name"].as_str().unwrap().ends_with(".torrent"));
    let bad_form = Form::new().part("file", Part::bytes(b"x".to_vec()).file_name("not.txt"));
    let bad = app
        .client
        .post(format!("{}/api/tmp-files", app.base))
        .multipart(bad_form)
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
}

#[tokio::test]
async fn 切换存储目录迁移文件并恢复默认() {
    let app = app().await;
    let row = upload(&app, "root", "迁移测试.txt", b"storage move test").await;
    let id = row[0]["id"].as_i64().unwrap();
    let custom = app._dir.path().join("custom-storage");
    let switched = json_request(
        &app,
        reqwest::Method::PUT,
        "/api/settings/storage-dir",
        json!({"dir": custom.to_string_lossy(), "moveFiles": true}),
    )
    .await;
    assert_eq!(switched.status(), 200);
    let switched: Value = switched.json().await.unwrap();
    assert_eq!(switched["changed"], true);
    assert!(switched["movedFiles"].as_i64().unwrap() >= 1);
    assert!(switched["storageDir"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .starts_with(&custom.to_string_lossy().replace('\\', "/")));

    let moved: Value = app
        .client
        .get(format!("{}/api/files/{id}", app.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(moved["path"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .starts_with(&custom.to_string_lossy().replace('\\', "/")));
    let bytes = app
        .client
        .get(format!("{}/api/files/{id}/download", app.base))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), b"storage move test");

    let same: Value = json_request(
        &app,
        reqwest::Method::PUT,
        "/api/settings/storage-dir",
        json!({"dir":custom.to_string_lossy()}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(same["changed"], false);
    let back: Value = json_request(
        &app,
        reqwest::Method::PUT,
        "/api/settings/storage-dir",
        json!({"dir":"", "moveFiles":true}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(back["changed"], true);
    assert!(back["storageDir"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .ends_with("/storage"));
    let restored: Value = app
        .client
        .get(format!("{}/api/files/{id}", app.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(restored["path"]
        .as_str()
        .unwrap()
        .replace('\\', "/")
        .ends_with("/storage/迁移测试.txt"));
}
