use chrono::{Duration, Utc};
use fnos_power_scheduler::{
    Result,
    schedule::Task,
    store::Store,
    system,
    wire::{self, Request},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{Arc, Mutex},
};

struct App {
    web: PathBuf,
    socket: Option<String>,
    gateway_uid: u32,
    simulation: Mutex<Option<Store>>,
}
impl App {
    fn control(&self, req: Value) -> Result<Value> {
        #[cfg(target_os = "linux")]
        if let Some(path) = &self.socket {
            use std::os::unix::net::UnixStream;
            let mut stream = UnixStream::connect(path)?;
            stream.set_read_timeout(Some(std::time::Duration::from_secs(45)))?;
            stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
            writeln!(stream, "{req}")?;
            let mut bytes = Vec::new();
            let mut limited = (&mut stream).take(1024 * 1024);
            limited.read_to_end(&mut bytes)?;
            let value: Value = serde_json::from_slice(&bytes)?;
            if let Some(e) = value["error"].as_str() {
                return Err(e.into());
            }
            return Ok(value["value"].clone());
        }
        let guard = self.simulation.lock().map_err(|_| "模拟数据库锁异常")?;
        let store = guard.as_ref().ok_or("未配置电源接口")?;
        let now = Utc::now();
        match req["operation"].as_str().ok_or("缺少操作")? {
            "state"=>store.view(now,json!({"mode":"simulation","available":true,"alarm":system::next_wake(&store.tasks()?,now),"message":"模拟模式：不会关机，也不会修改硬件闹钟"})),
            "save"=>{let t:Task=serde_json::from_value(req["task"].clone())?;store.save(t,now)?;Ok(json!({"ok":true}))},
            "delete"=>{store.delete(req["id"].as_str().ok_or("缺少 ID")?)?;Ok(json!({"ok":true}))},
            "cancel"=>{store.cancel(now)?;Ok(json!({"ok":true}))},
            _=>Err("无效操作".into()),
        }
    }
    fn route(&self, request: Request, peer: Option<u32>) -> Result<(u16, String, Vec<u8>)> {
        let real = self.socket.is_some();
        if real
            && (peer != Some(self.gateway_uid)
                || request.headers.get("x-trim-isadmin").map(String::as_str) != Some("true")
                || request
                    .headers
                    .get("x-trim-userid")
                    .is_none_or(|s| s.parse::<u32>().is_err()))
        {
            return Ok((
                403,
                "text/plain; charset=utf-8".into(),
                "仅允许通过飞牛网关登录的管理员访问".as_bytes().to_vec(),
            ));
        }
        if !real {
            let host = request
                .headers
                .get("host")
                .map(String::as_str)
                .unwrap_or("");
            if !["127.0.0.1:8787", "localhost:8787", "127.0.0.1", "localhost"].contains(&host) {
                return Err("仅允许本机访问模拟服务".into());
            }
        }
        if !["GET", "HEAD"].contains(&request.method.as_str())
            && request.headers.get("x-power-scheduler").map(String::as_str) != Some("1")
        {
            return Err("缺少操作校验头".into());
        }
        let path = request.path.split('?').next().unwrap_or("");
        let path = if real {
            path.strip_prefix("/app/fnos-power-scheduler/")
                .ok_or("无效网关路径")?
        } else {
            path.strip_prefix('/').ok_or("无效路径")?
        };
        let operation = match (request.method.as_str(), path) {
            ("GET", "api/state") => Some(json!({"operation":"state"})),
            ("POST", "api/tasks") => Some(
                json!({"operation":"save","task":serde_json::from_slice::<Value>(&request.body)?}),
            ),
            ("POST", "api/cancel") => Some(json!({"operation":"cancel"})),
            ("DELETE", p) if p.starts_with("api/tasks/") => {
                Some(json!({"operation":"delete","id":p.trim_start_matches("api/tasks/")}))
            }
            _ => None,
        };
        if let Some(op) = operation {
            return Ok((
                200,
                "application/json; charset=utf-8".into(),
                serde_json::to_vec(&self.control(op)?)?,
            ));
        }
        if !["GET", "HEAD"].contains(&request.method.as_str()) || path.starts_with("api/") {
            return Ok((404, "text/plain".into(), b"Not found".to_vec()));
        }
        let relative = if path.is_empty() { "index.html" } else { path };
        if relative.contains('\\') || relative.split('/').any(|p| p == ".." || p.starts_with('.')) {
            return Err("无效文件路径".into());
        }
        let root = fs::canonicalize(&self.web)?;
        let Ok(file) = fs::canonicalize(root.join(relative)) else {
            return Ok((404, "text/plain".into(), b"Not found".to_vec()));
        };
        if !file.starts_with(&root) || !file.is_file() {
            return Err("无效文件路径".into());
        }
        let mime = match file.extension().and_then(|s| s.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" => "text/javascript",
            "css" => "text/css",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            _ => "application/octet-stream",
        };
        Ok((
            200,
            mime.into(),
            if request.method == "HEAD" {
                Vec::new()
            } else {
                fs::read(file)?
            },
        ))
    }
    fn handle(&self, stream: &mut (impl Read + Write), peer: Option<u32>) {
        let result = wire::read_http(stream).and_then(|r| self.route(r, peer));
        let (status, mime, bytes) = match result {
            Ok(v) => v,
            Err(e) => (
                400,
                "application/json; charset=utf-8".into(),
                json!({"error":e.to_string()}).to_string().into_bytes(),
            ),
        };
        let _ = wire::respond(stream, status, &mime, &bytes);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |key: &str, default: &str| {
        args.windows(2)
            .find(|v| v[0] == key)
            .map(|v| v[1].clone())
            .unwrap_or(default.into())
    };
    let socket = arg("--socket", "");
    let power_socket = arg("--power-socket", "");
    if socket.is_empty() != power_socket.is_empty() {
        return Err("真实模式必须同时指定网关与电源 Socket".into());
    }
    let simulation = if socket.is_empty() {
        let data = PathBuf::from(arg("--data", "work/data"));
        fs::create_dir_all(&data)?;
        let s = Store::open(&data.join("scheduler.db"))?;
        s.set_meta(
            "protection_until",
            &(Utc::now() + Duration::minutes(2)).to_rfc3339(),
        )?;
        Some(s)
    } else {
        None
    };
    let app = Arc::new(App {
        web: arg("--web", "web/dist").into(),
        socket: if power_socket.is_empty() {
            None
        } else {
            Some(power_socket)
        },
        gateway_uid: arg("--gateway-uid", "0").parse()?,
        simulation: Mutex::new(simulation),
    });
    eprintln!(
        "电源计划 Rust 后台启动；真实电源模式={}；关机调度由 systemd timer 管理",
        !socket.is_empty()
    );
    // A fixed worker pool bounds resource use even with stalled client requests.
    if socket.is_empty() {
        let listener = Arc::new(TcpListener::bind("127.0.0.1:8787")?);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let listener = listener.clone();
                let app = app.clone();
                scope.spawn(move || {
                    for stream in listener.incoming() {
                        match stream {
                            Ok(mut s) => {
                                let _ =
                                    s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                                let _ =
                                    s.set_write_timeout(Some(std::time::Duration::from_secs(10)));
                                app.handle(&mut s, None);
                            }
                            Err(e) => eprintln!("{e}"),
                        }
                    }
                });
            }
        });
    } else {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            use std::os::unix::net::UnixListener;
            let listener = Arc::new(UnixListener::bind(&socket)?);
            fs::set_permissions(&socket, fs::Permissions::from_mode(0o666))?;
            std::thread::scope(|scope| {
                for _ in 0..4 {
                    let listener = listener.clone();
                    let app = app.clone();
                    scope.spawn(move || {
                        for stream in listener.incoming() {
                            match stream {
                                Ok(mut s) => {
                                    let peer = wire::peer_uid(&s).ok();
                                    let _ = s
                                        .set_read_timeout(Some(std::time::Duration::from_secs(10)));
                                    let _ = s.set_write_timeout(Some(
                                        std::time::Duration::from_secs(10),
                                    ));
                                    app.handle(&mut s, peer);
                                }
                                Err(e) => eprintln!("{e}"),
                            }
                        }
                    });
                }
            });
        }
        #[cfg(not(target_os = "linux"))]
        return Err("真实模式只支持 Linux".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_requires_peer_and_admin() {
        let app = App {
            web: PathBuf::new(),
            socket: Some("/not-used".into()),
            gateway_uid: 42,
            simulation: Mutex::new(None),
        };
        let request = |admin: &str| Request {
            method: "GET".into(),
            path: "/app/fnos-power-scheduler/".into(),
            headers: std::collections::BTreeMap::from([
                ("x-trim-userid".into(), "1".into()),
                ("x-trim-isadmin".into(), admin.into()),
            ]),
            body: Vec::new(),
        };
        assert_eq!(app.route(request("true"), Some(0)).unwrap().0, 403);
        assert_eq!(app.route(request("false"), Some(42)).unwrap().0, 403);
        assert_eq!(app.route(request("true"), None).unwrap().0, 403);
    }
    #[test]
    fn traversal_and_write_header() {
        let app = App {
            web: PathBuf::new(),
            socket: None,
            gateway_uid: 0,
            simulation: Mutex::new(None),
        };
        let r = |method: &str, path: &str| Request {
            method: method.into(),
            path: path.into(),
            headers: std::collections::BTreeMap::from([("host".into(), "localhost:8787".into())]),
            body: b"{}".to_vec(),
        };
        assert!(app.route(r("GET", "/../Cargo.toml"), None).is_err());
        assert!(app.route(r("POST", "/api/cancel"), None).is_err());
    }
}
