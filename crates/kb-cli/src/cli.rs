//! `cli.rs` 定义命令行“协议面”：
//! - 全局选项（输出格式、静默、verbose、配置路径）
//! - 子命令与参数模型
//!
//! 该文件不承载业务逻辑，只负责参数结构与帮助信息契约。

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

const HELP_OVERVIEW: &str = "These are common kb commands used in various situations:\n\nstart a working area\n  init        Create kb.md, .kb state and default ignore rules\n\nmanage references\n  add         Add file contents to the knowledge base\n  mv          Move or rename an alias\n  rm          Remove an alias from the working set\n  path        Show or update the path of an alias\n  chmod       Update permissions of an alias\n  list        List tracked aliases\n\nmaintain index state\n  check       Show index diff without writing files\n  update      Apply index diff and persist snapshot (use --strict for L2-L5 hard-fail)\n\nbuild agent memory\n  remember    Record an agent observation into the L0 episode timeline\n  correct     Record a user correction (highest-priority memory signal)\n  consolidate Distill accumulated episodes into agent-memory files\n\nretrieve and environment\n  ask         Retrieve relevant context for a question\n  serve       Serve the memory system over stdio JSON-RPC (MCP) for external agents\n  env         Show registered knowledge base roots\n  completion  Generate shell completion scripts\n";

// CLI 参数契约：全局选项 + 子命令集合。
// 这里的结构应与 docs/cli/command-manual.md 保持同步。
#[derive(Debug, Parser)]
#[command(
    name = "kb",
    bin_name = "kb",
    version,
    about = "KB CLI MVP command surface",
    override_usage = "kb [OPTIONS] <COMMAND>",
    after_help = HELP_OVERVIEW,
    disable_help_subcommand = true
)]
pub struct Cli {
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub output: OutputFormat,

    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub quiet: bool,

    #[arg(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
    pub verbose: u8,

    #[arg(long, global = true)]
    pub config: Option<String>,

    #[arg(long = "no-color", global = true, action = ArgAction::SetTrue)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    /// 面向用户的可读输出。
    Text,
    /// 面向机器消费的结构化 JSON 输出。
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AddMode {
    /// 直接引用原路径文件。
    Link,
    /// 复制文件到 `./.kb/raw/` 后再建立引用。
    Copy,
}

// 子命令参数模型：字段命名与可选性应与 docs 契约保持一致。
#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Init {
        name: Option<String>,
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
    Add {
        path: String,
        alias: Option<String>,
        #[arg(long, value_enum, default_value_t = AddMode::Link)]
        mode: AddMode,
        #[arg(long)]
        category: Option<String>,
    },
    Mv {
        alias: String,
        new_alias: String,
    },
    Rm {
        alias: String,
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
    Path {
        alias: String,
        new_path: Option<String>,
    },
    Chmod {
        alias: String,
        perms: String,
    },
    List {
        #[arg(long, action = ArgAction::SetTrue)]
        long: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        tree: bool,
        #[arg(long)]
        filter: Option<String>,
    },
    Check {
        #[arg(long, action = ArgAction::SetTrue)]
        raw_only: bool,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        top: Option<u32>,
    },
    Update {
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        prune: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        full: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        strict: bool,
        #[arg(long, action = ArgAction::SetTrue)]
        llm: bool,
    },
    Ask {
        question: String,
        #[arg(long)]
        top: Option<u32>,
        #[arg(long, alias = "llm", action = ArgAction::SetTrue)]
        answer: bool,
    },
    // Record an agent observation into the L0 episode timeline.
    // (description shown via HELP_OVERVIEW, matching the other commands)
    Remember {
        /// The fact / observation to remember.
        content: String,
        /// chunk_id(s) this observation references (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
        /// Self-assessed confidence in [0.0, 1.0]; defaults to 0.8.
        #[arg(long)]
        confidence: Option<f32>,
    },
    // Record a user correction (highest-priority memory signal).
    Correct {
        /// The corrected statement.
        content: String,
        /// chunk_id(s) this correction applies to (repeatable).
        #[arg(long = "ref")]
        refs: Vec<String>,
    },
    // Distill accumulated episodes into .kb/agent-memory/*.md memory files.
    Consolidate {
        /// Minimum episodes per topic before a memory file is produced.
        #[arg(long, default_value_t = 2)]
        min: usize,
        /// Plan only: show what would be written without touching disk.
        #[arg(long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
    Env {
        #[arg(long, action = ArgAction::SetTrue)]
        list: bool,
        /// 诊断模式：打印 LLM 配置解析路径（key/model/base_url 各自的来源），
        /// 含 keyring service/user 名，方便排查 "key 究竟从哪里读到"。
        #[arg(long, action = ArgAction::SetTrue)]
        doctor: bool,
    },
    // Serve the memory system over stdio JSON-RPC (MCP) for external agents.
    Serve {
        /// Speak the Model Context Protocol over stdio (the only transport for now).
        #[arg(long, action = ArgAction::SetTrue)]
        mcp: bool,
    },
    Completion {
        shell: clap_complete::Shell,
    },
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::{CommandFactory, Parser};

    #[test]
    fn help_contains_exact_command_set() {
        let help = Cli::command().render_long_help().to_string();
        for cmd in [
            "init",
            "add",
            "mv",
            "rm",
            "path",
            "chmod",
            "list",
            "check",
            "update",
            "ask",
            "remember",
            "correct",
            "consolidate",
            "serve",
            "env",
            "completion",
        ] {
            assert!(help.contains(cmd), "help missing command: {cmd}");
        }
    }

    #[test]
    fn add_parses_category_and_mode() {
        let cli = Cli::parse_from([
            "kb",
            "add",
            "./docs/guide.md",
            "guide",
            "--category",
            "raw",
            "--mode",
            "copy",
        ]);

        match cli.command {
            Commands::Add {
                path,
                alias,
                category,
                ..
            } => {
                assert_eq!(path, "./docs/guide.md");
                assert_eq!(alias.as_deref(), Some("guide"));
                assert_eq!(category.as_deref(), Some("raw"));
            }
            _ => panic!("expected add command"),
        }
    }

    #[test]
    fn help_usage_uses_kb_not_exe() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("Usage: kb [OPTIONS] <COMMAND>"));
    }

    #[test]
    fn help_contains_grouped_overview_sections() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("manage references"));
        assert!(help.contains("maintain index state"));
        assert!(help.contains("retrieve and environment"));
    }

    #[test]
    fn ask_accepts_llm_alias_for_answer_mode() {
        let cli = Cli::parse_from(["kb", "ask", "hello", "--llm"]);

        match cli.command {
            Commands::Ask {
                question, answer, ..
            } => {
                assert_eq!(question, "hello");
                assert!(answer);
            }
            _ => panic!("expected ask command"),
        }
    }

    #[test]
    fn serve_parses_mcp_flag() {
        let cli = Cli::parse_from(["kb", "serve", "--mcp"]);
        match cli.command {
            Commands::Serve { mcp } => assert!(mcp),
            _ => panic!("expected serve command"),
        }
    }
}
