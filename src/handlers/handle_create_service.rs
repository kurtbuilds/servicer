use clap::Parser;
use indoc::formatdoc;
use libc::LOG_AUTH;
use std::{env, path::PathBuf};
use tokio::fs;

use crate::{
    handlers::{
        handle_enable_service::handle_enable_service, handle_show_status::handle_show_status,
        handle_start_service::handle_start_service,
    },
    utils::{
        find_binary_path::find_binary_path,
        service_names::{get_full_service_name, get_service_file_path},
    },
};

/// Creates a new systemd service file.
///
/// # Arguments
///
/// * `path` - Create service for a file at this path
/// * `custom_name`
/// * `custom_interpreter`
/// * `env_vars`
/// * `internal_args`
///
#[derive(Parser, Debug)]
pub struct CreateArgs {
    /// Optional custom name for the service
    #[arg(short, long)]
    name: Option<String>,

    /// Working directory for the service
    #[arg(short, long)]
    directory: Option<String>,

    /// Print the service file that would be created, but don't create it
    /// can also think of it as "dry"
    #[arg(short = 'D', long)]
    debug: bool,

    /// Optional user for the service
    #[arg(short, long)]
    user: Option<String>,

    /// Start the service
    #[arg(short, long)]
    start: bool,

    /// Enable the service to start every time on boot. This doesn't immediately start the service, to do that run
    /// together with `start`
    #[arg(short, long)]
    enable: bool,

    /// Auto-restart on failure. Default false. You should edit the .service file for more advanced features.
    /// The service must be enabled for auto-restart to work.
    #[arg(short = 'r', long)]
    auto_restart: bool,

    /// Optional custom interpreter. Input can be the executable's name, eg `python3` or the full path
    /// `usr/bin/python3`. If no input is provided servicer will use the file extension to detect the interpreter.
    #[arg(short, long)]
    interpreter: Option<String>,

    /// Optional environment variables. To run `FOO=BAR node index.js` call `ser create index.js --env_vars "FOO=BAR"`
    #[arg(short = 'v', long)]
    env_vars: Vec<String>,

    /// Optional args passed to the file. Eg. to run `node index.js --foo bar` call `ser create index.js -- --foo bar`
    // #[arg(last = true)]
    command: Vec<String>,
}

pub async fn handle_create_service(args: CreateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.command.first().expect("No command provided");
    let path = PathBuf::from(path);

    let service_name = args
        .name
        .unwrap_or_else(|| path.file_name().unwrap().to_str().unwrap().to_string());

    let full_service_name = get_full_service_name(&service_name);

    // Create file if it doesn't exist
    let service_file_path = get_service_file_path(&full_service_name);
    let service_file_path_str = service_file_path.to_str().unwrap();

    if service_file_path.exists() {
        panic!("Service {service_name} already exists at {service_file_path_str}. Provide a custom name with --name or delete the existing service with `ser delete {service_name}");
    }

    let user = args
        .user
        .or(env::var("SUDO_USER").ok())
        .unwrap_or_else(|| env::var("USER").expect("USER is not set"));

    let mut interpreter = args
        .interpreter
        .or_else(|| get_interpreter(path.extension()));
    if let Some(i) = interpreter {
        let bin = find_binary_path(&i, &user)
            .await
            .unwrap()
            .expect("No binary for interpreter");
        interpreter = Some(bin);
    }

    let directory = if let Some(directory) = args.directory {
        fs::canonicalize(directory)
            .await
            .expect("Unknown directory")
    } else if interpreter.is_some() {
        let path = path.parent().unwrap();
        if path.to_str() == Some("") {
            env::current_dir().unwrap()
        } else {
            fs::canonicalize(path).await.unwrap()
        }
    } else {
        env::current_dir().unwrap()
    };
    let directory = directory.to_str().unwrap();

    let mut command = args.command;
    if let Some(interpreter) = interpreter {
        command.insert(0, interpreter);
    } else {
        if !path.is_file() {
            let bin = find_binary_path(path.to_str().unwrap(), &user)
                .await
                .unwrap();
            if let Some(bin) = bin {
                command[0] = bin;
            }
        }
    }
    let restart = args.auto_restart;
    let service_body = create_service_file(command, directory, &user, args.env_vars, restart);
    if args.debug {
        print!("{}", service_body)
    } else {
        fs::write(&service_file_path, service_body).await.unwrap();
        println!("Service {service_name} created at {service_file_path_str}. To start run `ser start {service_name}`");
        if args.start {
            handle_start_service(&service_name, false).await.unwrap();
        }
        if args.enable {
            handle_enable_service(&service_name, false).await.unwrap();
        }
        handle_show_status().await?;
    }
    Ok(())
}

/// Find the interpreter needed to execute a file with the given extension
///
/// # Arguments
///
/// * `extension`: The file extension
///
fn get_interpreter(extension: Option<&std::ffi::OsStr>) -> Option<String> {
    let extension = extension?;
    let extension_str = extension.to_str().expect("failed to stringify extension");
    let i = match extension_str {
        "js" => "node",
        "py" => "python3",
        _ => return None,
    };
    Some(i.to_string())
}

/// Creates a systemd service file at `/etc/systemd/system/{}.ser.service` and returns the unit name
fn create_service_file(
    command: Vec<String>,
    directory: &str,
    user: &str,
    env_vars: Vec<String>,
    auto_restart: bool,
) -> String {
    // This gets `root` instead of `hp` if sudo is used

    let mut command = command.join(" ");

    if auto_restart {
        command.push_str("\nRestart=always");
    }
    for var in env_vars {
        command.push_str(&format!("\nEnvironment={}", var));
    }
    formatdoc! {
        r#"
      # Generated with Servicer
      [Unit]
      After=network.target

      [Service]
      Type=simple
      User={user}

      WorkingDirectory={directory}
      ExecStart={command}

      [Install]
      WantedBy=multi-user.target
      "#
    }
}
