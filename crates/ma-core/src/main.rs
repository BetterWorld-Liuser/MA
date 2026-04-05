use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use ma::agent::{AgentConfig, AgentRunResult, AgentSession, DebugRound};
use ma::context::ConversationHistory;
use ma::provider::{OpenAiCompatibleClient, OpenAiCompatibleConfig};
use ma::storage::MaStorage;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let cwd = std::env::current_dir()?;
    let mut storage = MaStorage::open(&cwd)?;
    let cli_args = std::env::args().skip(1).collect::<Vec<_>>();
    let cli_request = (!cli_args.is_empty()).then(|| cli_args.join(" "));
    let config = AgentConfig {
        max_recent_turns: 10,
        ..AgentConfig::default()
    };

    let task = storage
        .list_tasks()?
        .into_iter()
        .next()
        .map(|task| storage.load_task(task.id))
        .transpose()?;

    let (task_id, mut session) = if let Some(task) = task {
        let task_id = task.task.id;
        (task_id, AgentSession::restore(config, task)?)
    } else {
        let task = storage.create_task("默认任务")?;
        let session = AgentSession::new(config, ConversationHistory::default(), [], cwd.clone())?;
        storage.save_task_state(task.id, &session.persisted_state())?;
        (task.id, session)
    };

    let provider = OpenAiCompatibleClient::new(OpenAiCompatibleConfig::from_env()?);
    let mut debug_enabled = false;
    let debug_logs = DebugLogs::new(cwd.join(".ma").join("debug"))?;

    if let Some(request) = cli_request {
        match session.handle_user_message(&provider, request).await {
            Ok(result) => {
                print_agent_result(&result, debug_enabled, &debug_logs)?;
                storage.save_task_state(task_id, &session.persisted_state())?;
            }
            Err(error) => {
                storage.save_task_state(task_id, &session.persisted_state())?;
                return Err(error);
            }
        }
        return Ok(());
    }

    println!("March MVP CLI");
    println!("Workspace: {}", cwd.display());
    println!(
        "Type a request and press Enter. Use /prompt to inspect context, /debug to toggle debug, /exit to quit.\n"
    );

    loop {
        print!("march> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }
        if input == "/exit" {
            break;
        }
        if input == "/prompt" {
            println!("{}", session.render_prompt());
            continue;
        }
        if input == "/debug" {
            debug_enabled = !debug_enabled;
            if debug_enabled {
                debug_logs.reset()?;
                if let Err(error) = open_debug_terminals(&debug_logs) {
                    eprintln!("\nFailed to open debug terminals: {error:#}\n");
                }
            }
            println!(
                "\nDebug panel {}\n",
                if debug_enabled { "enabled" } else { "disabled" }
            );
            continue;
        }

        match session.handle_user_message(&provider, input).await {
            Ok(result) => {
                print_agent_result(&result, debug_enabled, &debug_logs)?;
                storage.save_task_state(task_id, &session.persisted_state())?;
            }
            Err(error) => {
                eprintln!("\nError: {error:#}\n");
                storage.save_task_state(task_id, &session.persisted_state())?;
            }
        }
    }

    storage.save_task_state(task_id, &session.persisted_state())?;
    Ok(())
}

fn print_agent_result(
    result: &AgentRunResult,
    debug_enabled: bool,
    debug_logs: &DebugLogs,
) -> Result<()> {
    for final_message in &result.final_messages {
        println!("\n{}\n", final_message.message);
    }

    if debug_enabled {
        print_debug_rounds(&result.debug_rounds);
        debug_logs.write_rounds(&result.debug_rounds)?;
    }

    Ok(())
}

fn print_debug_rounds(rounds: &[DebugRound]) {
    if rounds.is_empty() {
        println!("--- Debug ---");
        println!("No provider rounds recorded.\n");
        return;
    }

    println!("--- Debug ---");
    for round in rounds {
        println!("Round {}", round.iteration);
        println!("[Context]");
        println!("{}", round.context_preview);
        println!("[Provider Request]");
        println!("{}", pretty_json_or_original(&round.provider_request_json));
        println!("[Provider Raw Response]");
        println!("{}", pretty_json_or_original(&round.provider_raw_response));

        if round.tool_calls.is_empty() {
            println!("[Tool Calls]");
            println!("(none)");
        } else {
            println!("[Tool Calls]");
            for tool_call in &round.tool_calls {
                println!(
                    "- {} {} {}",
                    tool_call.id, tool_call.name, tool_call.arguments_json
                );
            }
        }

        if round.tool_results.is_empty() {
            println!("[Tool Results]");
            println!("(none)");
        } else {
            println!("[Tool Results]");
            for result in &round.tool_results {
                println!("{}", result);
            }
        }

        println!();
    }
}

struct DebugLogs {
    dir: PathBuf,
    context_path: PathBuf,
    provider_path: PathBuf,
}

impl DebugLogs {
    fn new(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self {
            context_path: dir.join("context.log"),
            provider_path: dir.join("provider.log"),
            dir,
        })
    }

    fn reset(&self) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(&self.context_path, "March context debug log\n\n")?;
        fs::write(&self.provider_path, "March provider debug log\n\n")?;
        Ok(())
    }

    fn write_rounds(&self, rounds: &[DebugRound]) -> Result<()> {
        if rounds.is_empty() {
            return Ok(());
        }

        let mut context_output = String::new();
        let mut provider_output = String::new();

        for round in rounds {
            context_output.push_str(&format!("===== Round {} =====\n", round.iteration));
            context_output.push_str(&round.context_preview);
            context_output.push_str("\n\n");

            provider_output.push_str(&format!("===== Round {} =====\n", round.iteration));
            provider_output.push_str("[Provider Request]\n");
            provider_output.push_str(&pretty_json_or_original(&round.provider_request_json));
            provider_output.push_str("\n\n[Provider Raw Response]\n");
            provider_output.push_str(&pretty_json_or_original(&round.provider_raw_response));
            provider_output.push_str("\n\n[Tool Calls]\n");
            if round.tool_calls.is_empty() {
                provider_output.push_str("(none)\n");
            } else {
                for tool_call in &round.tool_calls {
                    provider_output.push_str(&format!(
                        "- {} {} {}\n",
                        tool_call.id, tool_call.name, tool_call.arguments_json
                    ));
                }
            }
            provider_output.push_str("\n[Tool Results]\n");
            if round.tool_results.is_empty() {
                provider_output.push_str("(none)\n");
            } else {
                for result in &round.tool_results {
                    provider_output.push_str(result);
                    provider_output.push('\n');
                }
            }
            provider_output.push('\n');
        }

        append_text(&self.context_path, &context_output)?;
        append_text(&self.provider_path, &provider_output)?;
        Ok(())
    }
}

fn append_text(path: &Path, text: &str) -> Result<()> {
    use std::io::Write as _;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    Ok(())
}

fn pretty_json_or_original(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| text.to_string())
}

#[cfg(windows)]
fn open_debug_terminals(debug_logs: &DebugLogs) -> Result<()> {
    open_debug_terminal("March Context Debug", &debug_logs.context_path)?;
    open_debug_terminal("March Provider Debug", &debug_logs.provider_path)?;
    Ok(())
}

#[cfg(not(windows))]
fn open_debug_terminals(_debug_logs: &DebugLogs) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn open_debug_terminal(title: &str, path: &Path) -> Result<()> {
    let path = path.display().to_string().replace('\'', "''");
    let command = format!(
        "start \"{title}\" powershell -NoExit -Command \"$Host.UI.RawUI.WindowTitle = '{title}'; Get-Content -Path '{path}' -Wait\""
    );
    Command::new("cmd").args(["/C", &command]).spawn()?;
    Ok(())
}
