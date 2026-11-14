//! 测试共享驱动：spawn 真实 rp-mcp 二进制（`CARGO_BIN_EXE_rp-mcp`，Cargo
//! 集成测试内建 env），以真实 MCP 客户端形态逐行驱动 stdio JSON-RPC。
//! rmcp 迁移批次（2026-11-14）：协议面由 rmcp 承载，本驱动只做裸 JSON-RPC
//! 行交换（按 id 匹配响应），不引入 rmcp client feature（红线：不拉客户端面）。
#![allow(dead_code)]

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

/// 常时测试令牌（三态令牌用例自行覆盖缺省 / 空值态）。
pub const TOKEN: &str = "test-token-e2e";

/// 运行中的 rp-mcp 子进程（stdout 逐行读，stdin 行写）。
pub struct McpProc {
    pub child: Child,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: std::io::Lines<BufReader<ChildStdout>>,
    next_id: i64,
    /// 不与本测试 id 匹配的响应行（坏 JSON 无响应；其他用例如握手响应
    /// 由 request 匹配消费，残余行落此带外区供断言）。
    pub sideband: Vec<Value>,
}

impl McpProc {
    /// spawn rp-mcp（默认：带令牌、RP_RUN_DB=none 关停执行面、RP_PLUGIN_DB=none；
    /// `env` 覆盖同名变量，重复键后者生效）。工作目录继承测试进程
    /// （crates/mcp-server）；需仓库根 CWD（python worker 相对路径）的用例
    /// 走 [`McpProc::spawn_in`]。
    pub fn spawn(env: &[(&str, &str)]) -> Self {
        Self::spawn_in(env, None)
    }

    /// spawn rp-mcp 并指定工作目录（仓库根执行纪律：python worker 命令为
    /// 仓库根相对路径，bin 层 working_dir = 进程 CWD）。
    pub fn spawn_in(env: &[(&str, &str)], dir: Option<&std::path::Path>) -> Self {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_rp-mcp"));
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        cmd.env("RP_MCP_TOKEN", TOKEN)
            .env("RP_RUN_DB", "none")
            .env("RP_PLUGIN_DB", "none");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn().expect("spawn rp-mcp binary");
        let stdin = child.stdin.take().expect("rp-mcp stdin");
        let stdout = child.stdout.take().expect("rp-mcp stdout");
        Self {
            child,
            stdin: Some(BufWriter::new(stdin)),
            stdout: BufReader::new(stdout).lines(),
            next_id: 100,
            sideband: Vec::new(),
        }
    }

    /// 裸写一行（不读响应；通知 / 坏行 / 需要手工配对时用）。
    pub fn send_raw(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin closed");
        writeln!(stdin, "{line}").expect("write stdin line");
        stdin.flush().expect("flush stdin");
    }

    /// 发请求并读回匹配 id 的响应（带外响应行落 sideband）。
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.request_with_id(method, params, id)
    }

    /// 指定 id 发请求并读回匹配 id 的响应。
    pub fn request_with_id(&mut self, method: &str, params: Value, id: i64) -> Value {
        let line = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
        self.send_raw(&line);
        loop {
            let line = self
                .stdout
                .next()
                .expect("rp-mcp closed stdout before matching response")
                .expect("read stdout line");
            let msg: Value = serde_json::from_str(&line).expect("stdout line is valid JSON");
            if msg.get("id").and_then(Value::as_i64) == Some(id) && msg.get("method").is_none() {
                return msg;
            }
            self.sideband.push(msg);
        }
    }

    /// 标准 initialize 握手（2024-11-05）+ notifications/initialized。
    /// 返回 initialize 响应。
    pub fn handshake(&mut self, protocol_version: &str) -> Value {
        let reply = self.request(
            "initialize",
            json!({
                "protocolVersion": protocol_version,
                "capabilities": {},
                "clientInfo": {"name": "mcp-protocol-test", "version": "0"}
            }),
        );
        self.send_raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        reply
    }

    /// tools/call 便捷面（返回完整 JSON-RPC 响应）。
    pub fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": args}))
    }

    /// 关闭 stdin（EOF）并等待进程退出，返回退出码。
    pub fn wait_exit(&mut self) -> i32 {
        // drop stdin 写端 → rmcp transport 读到 EOF → 服务收尾退出 0。
        drop(self.stdin.take());
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => return status.code().unwrap_or(-1),
                None => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
    }
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 临时库路径（每测试独立，退出清理交给调用方或系统临时目录）。
pub fn tmp_db(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "rp_mcp_test_{}_{}_{}.db",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    path
}
