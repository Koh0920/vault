#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn load_dotenv() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate_paths = [
        manifest_dir.parent().map(|path| path.join(".env")),
        Some(manifest_dir.join(".env")),
    ];

    let mut loaded = false;
    for path in candidate_paths.into_iter().flatten() {
        if path.exists() {
            // println!("--- Debug: Reading file at {} ---", path.display());
            // match std::fs::read_to_string(&path) {
            //     Ok(content) => {
            //         for (i, line) in content.lines().enumerate() {
            //             println!("{:3}: |{}|", i + 1, line);
            //         }
            //     }
            //     Err(e) => println!("Could not read file: {}", e),
            // }

            match dotenvy::from_path_override(&path) {
                Ok(_) => {
                    println!("[Success] Loaded: {}", path.display());
                    loaded = true;
                }
                Err(e) => eprintln!("[failed] Failed to load {}: {}", path.display(), e),
            }
            break;
        }
    }
    if !loaded {
        eprintln!("[warning] No .env file found in candidate paths.");
    }
}

fn main() {
    load_dotenv();

    if std::env::var("ATO_GUEST_MODE").ok().as_deref() == Some("1") {
        byok_encrypted_r2_drop_lib::run_guest_server().expect("failed to run guest server");
    } else {
        byok_encrypted_r2_drop_lib::run();
    }
}
