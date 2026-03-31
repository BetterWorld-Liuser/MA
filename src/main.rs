use std::time::{Duration, SystemTime};

use anyhow::Result;
use ma::agent::{AgentConfig, AgentSession, CommandRequest, CommandShell};
use ma::context::{ConversationHistory, DisplayTurn, Role, ToolSummary};
use ma::provider::{OpenAiCompatibleClient, OpenAiCompatibleConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let cwd = std::env::current_dir()?;
    let history = ConversationHistory::new(vec![
        DisplayTurn {
            role: Role::User,
            content: "帮我搭一个最小 Rust 项目骨架".to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::UNIX_EPOCH,
        },
        DisplayTurn {
            role: Role::Assistant,
            content: "已经初始化 cargo 项目，并开始实现上下文管理层。".to_string(),
            tool_calls: vec![ToolSummary {
                name: "cargo init".to_string(),
                summary: "创建了 ma 的二进制项目骨架".to_string(),
            }],
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        },
    ]);
    let mut session = AgentSession::new(
        AgentConfig {
            max_recent_turns: 4,
            ..AgentConfig::default()
        },
        history,
        [
            cwd.join("src").join("main.rs"),
            cwd.join("src").join("context.rs"),
            cwd.join("src").join("watcher.rs"),
        ],
    )?;

    session.add_user_turn("请展示当前最小 agent loop 的状态");
    session.write_note("target", "展示当前最小 agent loop 的状态");
    session.add_assistant_turn(
        "已根据 watcher 快照和最近对话构建 prompt，并准备执行一次命令行读取。",
        Vec::new(),
    );

    let execution = session.run_command(CommandRequest {
        command: preview_command().to_string(),
        shell: preview_shell(),
    })?;

    let provider_reply = match OpenAiCompatibleConfig::from_env() {
        Ok(config) => {
            let client = OpenAiCompatibleClient::new(config);
            Some(session.generate_assistant_reply(&client).await?)
        }
        Err(_) => None,
    };

    let context = session.build_context();

    println!("Ma agent loop ready.");
    println!("Recent chat kept: {}", context.recent_chat.len());
    println!("Notes kept: {}", context.notes.len());
    println!("Open files in prompt order:");
    for snapshot in context.open_files_in_prompt_order() {
        println!(
            "- {} ({:?}, changed={})",
            snapshot.path.display(),
            snapshot.last_modified_by,
            snapshot.has_changed_since_watch
        );
    }

    println!(
        "\nLast command shell: {:?}\nLast command exit code: {}",
        execution.shell, execution.exit_code
    );
    if !execution.stdout.is_empty() {
        println!("Last command stdout:\n{}", execution.stdout);
    }

    if let Some(reply) = provider_reply {
        println!("\nProvider reply:\n{}", reply);
    } else {
        println!(
            "\nProvider reply: skipped (set MA_OPENAI_BASE_URL / MA_OPENAI_API_KEY / MA_OPENAI_MODEL to enable)"
        );
    }

    println!("\nPrompt preview:\n{}", session.render_prompt());

    Ok(())
}

#[cfg(windows)]
fn preview_command() -> &'static str {
    "Get-Content -TotalCount 3 src\\main.rs"
}

#[cfg(not(windows))]
fn preview_command() -> &'static str {
    "head -n 3 src/main.rs"
}

#[cfg(windows)]
fn preview_shell() -> CommandShell {
    CommandShell::PowerShell
}

#[cfg(not(windows))]
fn preview_shell() -> CommandShell {
    CommandShell::Sh
}
