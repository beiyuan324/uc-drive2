use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::to_bytes;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

use uc_drive2_lib::backend::gopeed::GopeedSpawner;
use uc_drive2_lib::backend::routes;
use uc_drive2_lib::backend::winutil::{terminate_pid, CREATE_NO_WINDOW};
use uc_drive2_lib::backend::{self, AppState};

#[derive(Clone)]
struct MockState {
    token: String,
    tasks: Arc<Mutex<HashMap<String, Value>>>,
    config: Arc<Mutex<Value>>,
    sequence: Arc<Mutex<u64>>,
    list_calls: Arc<Mutex<u64>>,
}

#[derive(Clone)]
struct MockSpawner {
    state: Arc<Mutex<Option<MockState>>>,
}

impl MockSpawner {
    fn new() -> Self {
        MockSpawner {
            state: Arc::new(Mutex::new(None)),
        }
    }

    fn tasks(&self) -> Arc<Mutex<HashMap<String, Value>>> {
        self.state.lock().unwrap().as_ref().unwrap().tasks.clone()
    }

    fn list_calls(&self) -> u64 {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .list_calls
            .lock()
            .unwrap()
            .to_owned()
    }
}

impl GopeedSpawner for MockSpawner {
    fn spawn(
        &self,
        host: &str,
        port: u16,
        token: &str,
        _storage_dir: &std::path::Path,
    ) -> std::io::Result<Child> {
        let state = MockState {
            token: token.to_string(),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(json!({
                "maxRunning": 3,
                "protocolConfig": {"http": {"connections": 500}}
            }))),
            sequence: Arc::new(Mutex::new(0)),
            list_calls: Arc::new(Mutex::new(0)),
        };
        *self.state.lock().unwrap() = Some(state.clone());
        let router = Router::new().fallback(mock_handler).with_state(state);
        let listener = std::net::TcpListener::bind((host, port))?;
        listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener)?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let mut command = Command::new("cmd.exe");
        command
            .args(["/C", "ping -n 3600 127.0.0.1 > nul"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.spawn()
    }
}

async fn mock_handler(
    State(state): State<MockState>,
    request: Request<axum::body::Body>,
) -> Response {
    let authorized = request
        .headers()
        .get("x-api-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == state.token)
        .unwrap_or(false);
    if !authorized {
        return response(1001, "unauthorized", Value::Null);
    }
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let query = request.uri().query().unwrap_or("").to_string();
    let bytes = to_bytes(request.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap_or_default();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    if method == reqwest::Method::GET && path == "/api/v1/info" {
        return response(
            0,
            "",
            json!({"version":"mock","runtime":"go","os":"windows","arch":"amd64"}),
        );
    }
    if method == reqwest::Method::GET && path == "/api/v1/config" {
        return response(0, "", state.config.lock().unwrap().clone());
    }
    if method == reqwest::Method::PUT && path == "/api/v1/config" {
        *state.config.lock().unwrap() = body;
        return response(0, "", Value::Null);
    }
    if method == reqwest::Method::POST && path == "/api/v1/tasks" {
        let mut sequence = state.sequence.lock().unwrap();
        *sequence += 1;
        let id = format!("mock-{sequence}");
        let url = body["req"]["url"].as_str().unwrap_or("mock");
        let name = url.rsplit('/').next().unwrap_or("mock").to_string();
        let task = json!({
            "id": id,
            "name": name,
            "status": "ready",
            "size": 100,
            "progress": {"speed": 0, "downloaded": 0},
            "meta": {
                "req": body["req"].clone(),
                "res": {"name":"", "size":100, "files":[{"name":"f.bin","path":"","size":100}]},
                "opts": body["opts"].clone()
            }
        });
        state.tasks.lock().unwrap().insert(id.clone(), task);
        return response(0, "", json!(id));
    }
    if method == reqwest::Method::GET && path == "/api/v1/tasks" {
        *state.list_calls.lock().unwrap() += 1;
        return response(
            0,
            "",
            Value::Array(state.tasks.lock().unwrap().values().cloned().collect()),
        );
    }
    if let Some(id) = path.strip_prefix("/api/v1/tasks/") {
        if method == reqwest::Method::GET {
            let value = state.tasks.lock().unwrap().get(id).cloned();
            return match value {
                Some(value) => response(0, "", value),
                None => response(2001, "task not found", Value::Null),
            };
        }
        if method == reqwest::Method::PUT && (id.ends_with("/pause") || id.ends_with("/continue")) {
            let (task_id, status) = if let Some(task_id) = id.strip_suffix("/pause") {
                (task_id, "pause")
            } else {
                (id.strip_suffix("/continue").unwrap_or(id), "running")
            };
            let mut tasks = state.tasks.lock().unwrap();
            if let Some(task) = tasks.get_mut(task_id) {
                task["status"] = json!(status);
                return response(0, "", Value::Null);
            }
            return response(2001, "task not found", Value::Null);
        }
        if method == reqwest::Method::DELETE {
            state
                .tasks
                .lock()
                .unwrap()
                .remove(id.split('?').next().unwrap_or(id));
            return response(0, "", Value::Null);
        }
    }
    let _ = query;
    response(1000, "not found", Value::Null)
}

fn response(code: i64, msg: &str, data: Value) -> Response {
    (
        StatusCode::OK,
        Json(json!({"code":code,"msg":msg,"data":data})),
    )
        .into_response()
}

struct TestApp {
    _dir: TempDir,
    pub state: Arc<AppState>,
    pub base: String,
    pub mock: Arc<MockSpawner>,
    server: JoinHandle<Result<(), std::io::Error>>,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        if let Some(pid) = self.state.gopeed.pid() {
            terminate_pid(pid);
        }
        self.server.abort();
    }
}

async fn app() -> TestApp {
    app_with_poll(Duration::from_millis(50)).await
}

async fn app_with_poll(poll_interval: Duration) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let mock = Arc::new(MockSpawner::new());
    let mut options = backend::StartOptions::new(
        dir.path().to_path_buf(),
        None,
        PathBuf::from("mock-gopeed-web.exe"),
    );
    options.spawner = Some(mock.clone());
    options.poll_interval = poll_interval;
    let state = backend::build_state(&options).unwrap();
    state.tasks.spawn_loops();
    state.gopeed.start().await.unwrap();
    let router = routes::build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move { axum::serve(listener, router).await });
    TestApp {
        _dir: dir,
        state,
        base: format!("http://127.0.0.1:{port}"),
        mock,
        server,
    }
}

#[tokio::test]
async fn gopeed_任务管理配置和连接数() {
    let app = app().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let row: Value = client
        .post(format!("{}/api/tasks", app.base))
        .json(&json!({"source":"url","url":"http://example.com/data.bin"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(row["gopeed_id"].as_str().unwrap().starts_with("mock-"));
    assert_eq!(row["status"], "queued");
    let gid = row["gopeed_id"].as_str().unwrap().to_string();
    assert!(app.mock.tasks().lock().unwrap()[&gid]["meta"]["opts"]
        .get("extra")
        .is_none());

    let paused = client
        .post(format!("{}/api/tasks/{}/pause", app.base, row["id"]))
        .send()
        .await
        .unwrap();
    assert_eq!(paused.status(), 200);
    assert_eq!(paused.json::<Value>().await.unwrap()["status"], "paused");
    let resumed = client
        .post(format!("{}/api/tasks/{}/resume", app.base, row["id"]))
        .send()
        .await
        .unwrap();
    assert_eq!(resumed.status(), 200);
    assert_eq!(resumed.json::<Value>().await.unwrap()["status"], "running");

    let config = client
        .put(format!("{}/api/tasks/config", app.base))
        .json(&json!({"ucConnections":600,"httpConnections":64,"maxRunning":4}))
        .send()
        .await
        .unwrap();
    assert_eq!(config.status(), 200);
    let config: Value = config.json().await.unwrap();
    assert_eq!(config["ucConnections"], 600);
    assert_eq!(config["httpConnections"], 64);
    assert_eq!(config["maxRunning"], 4);
    assert_eq!(
        app.mock
            .state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .config
            .lock()
            .unwrap()["maxRunning"],
        4
    );

    let uc: Value = client
        .post(format!("{}/api/tasks", app.base))
        .json(&json!({"source":"uc","url":"http://example.com/uc.bin"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let uc_gid = uc["gopeed_id"].as_str().unwrap();
    assert_eq!(
        app.mock.tasks().lock().unwrap()[uc_gid]["meta"]["opts"]["extra"]["connections"],
        600
    );

    let normal: Value = client
        .post(format!("{}/api/tasks", app.base))
        .json(&json!({"source":"url","url":"http://example.com/normal.bin"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let normal_gid = normal["gopeed_id"].as_str().unwrap();
    assert_eq!(
        app.mock.tasks().lock().unwrap()[normal_gid]["meta"]["opts"]["extra"]["connections"],
        64
    );

    let deleted = client
        .post(format!("{}/api/tasks/{}/delete", app.base, row["id"]))
        .json(&json!({"force":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    assert!(app
        .state
        .tasks
        .get(row["id"].as_i64().unwrap())
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn 任务同步进度速度总大小和完成登记() {
    let app = app_with_poll(Duration::from_secs(5)).await;
    let row = app
        .state
        .tasks
        .create(backend::tasks::CreateTaskParams {
            source: "url".to_string(),
            url: Some("http://example.com/speed.bin".to_string()),
            torrent_id: None,
            torrent_name: None,
            filename: None,
            headers: None,
            uc: None,
            connections: None,
        })
        .await
        .unwrap();
    let gid = row.gopeed_id.clone();
    let first = json!({"id":gid,"name":"speed.bin","status":"ready","size":2048,"progress":{"speed":56,"downloaded":1024}});
    app.state.tasks.sync_from_gopeed(vec![first]).await.unwrap();
    let initial = app.state.tasks.get(row.id).unwrap().unwrap();
    assert_eq!(initial.progress, 50.0);
    assert_eq!(initial.metadata["total"], 2048);
    tokio::time::sleep(Duration::from_millis(650)).await;
    let second = json!({"id":gid,"name":"speed.bin","status":"ready","size":2048,"progress":{"speed":56*1024,"downloaded":2048}});
    app.state
        .tasks
        .sync_from_gopeed(vec![second])
        .await
        .unwrap();
    let updated = app.state.tasks.get(row.id).unwrap().unwrap();
    assert_eq!(updated.progress, 100.0);
    assert!(updated.speed > 0);

    // 完成登记：准备单文件，必须移动到存储根并置 done。
    let done = app
        .state
        .tasks
        .create(backend::tasks::CreateTaskParams {
            source: "url".to_string(),
            url: Some("http://example.com/hello.txt".to_string()),
            torrent_id: None,
            torrent_name: None,
            filename: None,
            headers: None,
            uc: None,
            connections: None,
        })
        .await
        .unwrap();
    let target = PathBuf::from(&done.target_dir);
    std::fs::write(target.join("hello.txt"), b"downloaded content").unwrap();
    app.state.tasks.sync_from_gopeed(vec![json!({"id":done.gopeed_id,"name":"hello.txt","status":"done","size":18,"progress":{"downloaded":18}})]).await.unwrap();
    let final_row = app.state.tasks.get(done.id).unwrap().unwrap();
    assert_eq!(final_row.status, "done");
    assert_eq!(final_row.progress, 100.0);
    assert!(app.state.storage.get().join("hello.txt").exists());
}

#[tokio::test]
async fn torrent_临时上传非法链接和空闲轮询() {
    let app = app().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let tmp = client
        .post(format!("{}/api/tmp-files", app.base))
        .multipart(Form::new().part(
            "file",
            Part::bytes(b"d8:announce0e".to_vec()).file_name("sample.torrent"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(tmp.status(), 201);
    let tmp: Value = tmp.json().await.unwrap();
    let tmp_name = tmp["name"].as_str().unwrap().to_string();
    assert!(app
        .state
        .paths
        .data_dir
        .join("tmp/torrents")
        .join(&tmp_name)
        .exists());

    let task: Value = client
        .post(format!("{}/api/tasks", app.base))
        .json(&json!({"source":"torrent","torrentName":tmp_name}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(task["gopeed_id"].as_str().is_some());
    assert!(!app
        .state
        .paths
        .data_dir
        .join("tmp/torrents")
        .join(tmp_name)
        .exists());

    let bad = client
        .post(format!("{}/api/tasks", app.base))
        .json(&json!({"source":"url","url":"not-a-url"}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 500);
    let before = app.mock.list_calls();
    tokio::time::sleep(Duration::from_millis(180)).await;
    // 由于有 queued 任务，此时会轮询；删除它后应回到空闲。
    let id = task["id"].as_i64().unwrap();
    client
        .post(format!("{}/api/tasks/{id}/delete", app.base))
        .json(&json!({"force":true}))
        .send()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(180)).await;
    assert!(app.mock.list_calls() >= before);
}

#[tokio::test]
async fn gopeed_异常退出自动换端口重启() {
    let app = app().await;
    let old_port = app.state.gopeed.port().unwrap();
    let pid = app.state.gopeed.pid().unwrap();
    terminate_pid(pid);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        if app.state.gopeed.ready() && app.state.gopeed.port() != Some(old_port) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(app.state.gopeed.ready());
    assert_ne!(app.state.gopeed.port(), Some(old_port));
}
