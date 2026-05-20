//! docx-smart-format engine (Rust rewrite).
//!
//! Targets the converged surface only: `analyze` + `build` (FormatPlan).
//! Legacy `apply`/`render` stay on the .NET engine (zero capability loss).

mod analyze;
mod autoformat;
mod docx;
mod plan;
mod presets;
mod refs;

use std::collections::HashMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return ExitCode::from(1);
    }

    let command = args[0].to_ascii_lowercase();
    let options = parse_options(&args[1..]);

    match command.as_str() {
        "build" => run_build(&options),
        "analyze" => {
            let input = match options.get("input") {
                Some(i) => i,
                None => {
                    eprintln!("缺少参数 --input");
                    return ExitCode::from(1);
                }
            };
            match analyze::analyze(input, options.get("output").map(|s| s.as_str())) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("analyze 失败: {e}");
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("不支持的命令: {other}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn run_build(options: &HashMap<String, String>) -> ExitCode {
    let plan_path = match options.get("plan") {
        Some(p) => p,
        None => {
            eprintln!("缺少参数 --plan");
            return ExitCode::from(1);
        }
    };
    let output = match options.get("output") {
        Some(o) => o,
        None => {
            eprintln!("缺少参数 --output");
            return ExitCode::from(1);
        }
    };

    let source_map = match options.get("source") {
        Some(src) => match analyze::source_blocks(src) {
            Ok(blocks) => Some(
                blocks
                    .into_iter()
                    .map(|b| (b.path.clone(), b))
                    .collect::<std::collections::HashMap<_, _>>(),
            ),
            Err(e) => {
                eprintln!("--source 分析失败: {e}");
                return ExitCode::from(1);
            }
        },
        None => None,
    };

    let json = match std::fs::read_to_string(plan_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("无法读取 {plan_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let plan: plan::FormatPlan = match serde_json::from_str(&json) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("FormatPlan 解析失败: {e}");
            return ExitCode::from(1);
        }
    };

    let normalize_refs = match options.get("normalize-references") {
        None => false,
        Some(v) => !matches!(v.trim().to_ascii_lowercase().as_str(), "false" | "off" | "0"),
    };

    let template_path = options.get("template").map(|s| s.as_str());

    // If the plan references paragraph styles via format.styleId but no
    // --template is supplied, those references are dangling and Word will
    // fall back to default Normal rendering. Warn loudly -- this is the
    // single most common user trip-wire.
    if template_path.is_none() {
        let style_refs: Vec<&str> = plan
            .blocks
            .iter()
            .filter_map(|b| b.format.as_ref()?.style_id.as_deref())
            .filter(|s| !s.is_empty())
            .collect();
        if !style_refs.is_empty() {
            eprintln!(
                "警告: plan 里有 {} 处 format.styleId 引用 ({})，但未提供 --template <docx>。\n\
                 引擎不会复制任何 word/styles.xml 进输出，<w:pStyle> 将是 dangling 引用，\n\
                 Word 渲染时会回退到默认 Normal 样式。如需让样式生效，请补上 \n\
                 --template <docx>，例如 --template samples\\template.docx。",
                style_refs.len(),
                {
                    let mut uniq: Vec<&str> = style_refs.iter().copied().collect();
                    uniq.sort();
                    uniq.dedup();
                    uniq.join(", ")
                }
            );
        }
    }

    match docx::build(&plan, output, source_map.as_ref(), normalize_refs, template_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("写回失败: {e}");
            ExitCode::from(1)
        }
    }
}

/// Mirrors the .NET ParseOptions: `--key value`, flags become empty string.
fn parse_options(args: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut pending: Option<String> = None;
    for arg in args {
        if let Some(key) = arg.strip_prefix("--") {
            map.entry(key.to_string()).or_insert_with(String::new);
            pending = Some(key.to_string());
        } else if let Some(key) = pending.take() {
            map.insert(key, arg.clone());
        }
    }
    map
}

fn print_usage() {
    eprintln!("用法:");
    eprintln!("  build --plan <format-plan.json> --output <result.docx> [--source <docx>] [--template <docx>] [--normalize-references]");
    eprintln!("  analyze --input <docx> --output <analysis.json>");
}
