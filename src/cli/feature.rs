use crate::api::schema::{
    EmptyParams, FeatureAssignPaneParams, FeatureCreateParams, Method, Request,
};

const USAGE: &str = "herdr feature commands:
  herdr feature list
  herdr feature create <name>
  herdr feature join <name> [--create] [--pane <pane_id>]";

pub(super) fn run_feature_command(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(|arg| arg.as_str()) {
        Some("list") => super::runtime::feature_list(),
        Some("create") => feature_create(&args[1..]),
        Some("join") => feature_join(&args[1..]),
        Some("help" | "--help" | "-h") => {
            eprintln!("{USAGE}");
            Ok(0)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(2)
        }
    }
}

fn feature_create(args: &[String]) -> std::io::Result<i32> {
    let name = args.join(" ");
    if name.trim().is_empty() {
        eprintln!("usage: herdr feature create <name>");
        return Ok(2);
    }
    super::runtime::feature_create(FeatureCreateParams { name })
}

struct JoinArgs {
    name: String,
    create: bool,
    pane_id: Option<String>,
}

fn parse_join_args(args: &[String]) -> Result<JoinArgs, String> {
    let mut words = Vec::new();
    let mut create = false;
    let mut pane_id = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--create" => create = true,
            "--pane" => {
                index += 1;
                pane_id = Some(args.get(index).ok_or("missing value for --pane")?.clone());
            }
            word => words.push(word.to_string()),
        }
        index += 1;
    }
    let name = words.join(" ");
    if name.trim().is_empty() {
        return Err("usage: herdr feature join <name> [--create] [--pane <pane_id>]".into());
    }
    Ok(JoinArgs {
        name,
        create,
        pane_id,
    })
}

/// Files a pane (the caller's own by default) under the feature with this name,
/// creating the feature first when `--create` is given and none matches.
fn feature_join(args: &[String]) -> std::io::Result<i32> {
    let args = match parse_join_args(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return Ok(2);
        }
    };
    let Some(pane_id) = args.pane_id.or_else(super::target::caller_pane_id) else {
        eprintln!("no pane to join: run inside a Herdr pane or pass --pane <pane_id>");
        return Ok(2);
    };
    let (feature_id, created) = match find_feature(&args.name)? {
        Some(feature_id) => (feature_id, false),
        None if args.create => match create_feature(&args.name)? {
            Ok(feature_id) => (feature_id, true),
            Err(response) => return super::print_response(&response),
        },
        None => {
            eprintln!(
                "no feature named {:?}; pass --create to create it",
                args.name.trim()
            );
            return Ok(1);
        }
    };
    let response = request(
        "cli:feature:join",
        Method::FeatureAssignPane(FeatureAssignPaneParams {
            pane_id: pane_id.clone(),
            feature_id: Some(feature_id.clone()),
            index: None,
        }),
    )?;
    if response.get("error").is_some() {
        return super::print_response(&response);
    }
    super::print_response(&serde_json::json!({
        "id": "cli:feature:join",
        "result": {
            "type": "feature_joined",
            "feature_id": feature_id,
            "name": args.name.trim(),
            "pane_id": pane_id,
            "created": created,
        }
    }))
}

fn request(id: &str, method: Method) -> std::io::Result<serde_json::Value> {
    super::send_request(&Request {
        id: id.into(),
        method,
    })
}

/// The id of the feature whose name matches, ignoring case and surrounding spaces.
fn find_feature(name: &str) -> std::io::Result<Option<String>> {
    let response = request(
        "cli:feature:list",
        Method::FeatureList(EmptyParams::default()),
    )?;
    let wanted = name.trim().to_lowercase();
    Ok(response["result"]["features"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|feature| {
            feature["name"]
                .as_str()
                .map(|name| name.trim().to_lowercase())
                == Some(wanted.clone())
        })
        .and_then(|feature| feature["feature_id"].as_str().map(str::to_string)))
}

/// The new feature's id, or the error response to print.
fn create_feature(name: &str) -> std::io::Result<Result<String, serde_json::Value>> {
    let response = request(
        "cli:feature:create",
        Method::FeatureCreate(FeatureCreateParams { name: name.into() }),
    )?;
    Ok(response["result"]["feature_id"]
        .as_str()
        .map(str::to_string)
        .ok_or(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn join_args_accept_multi_word_names_and_flags_anywhere() {
        let args =
            parse_join_args(&strings(&["live", "--create", "subs", "--pane", "w1:p2"])).unwrap();

        assert_eq!(args.name, "live subs");
        assert!(args.create);
        assert_eq!(args.pane_id.as_deref(), Some("w1:p2"));
    }

    #[test]
    fn join_args_require_a_name() {
        assert!(parse_join_args(&strings(&["--create"])).is_err());
        assert!(parse_join_args(&strings(&["x", "--pane"])).is_err());
    }
}
