use crate::api::schema::FeatureCreateParams;

pub(super) fn run_feature_command(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(|arg| arg.as_str()) {
        Some("create") => feature_create(&args[1..]),
        Some("help" | "--help" | "-h") => {
            print_feature_help();
            Ok(0)
        }
        _ => {
            print_feature_help();
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

fn print_feature_help() {
    eprintln!("herdr feature commands:");
    eprintln!("  herdr feature create <name>");
}
