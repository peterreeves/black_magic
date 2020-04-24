//! # Install
//! To install this program, navigate to this project's root and run:
//! ```
//! cargo install --path .
//! ```
//! See https://doc.rust-lang.org/cargo/commands/cargo-install.html for more details.
//! 
//! # Details
//! For details reguarding compilation, see:
//! https://gitlab.com/rust_musl_docker/image
//! Updating the image tag may be required from time to time.
//! https://gitlab.com/rust_musl_docker/image/container_registry/
//! 

use clap::App;
use clap::Arg;
use std::env;
use std::fs;
use std::process::Command;
use home;

const USAGE: &str = r#"
    Black Magic

    This is for building rust projects. It produces zips for AWS Lambda, or 'FROM scratch` docker images.
    You will need to have docker installed.
    It should work on Windows, Linux, and Mac. Getting builds working on Windows was the main reason for this project.
    This should only be run on projects that already compile, or at least pass 'cargo check'.
    Builds can be extremely slow, debugging will be paniful.

    Navigate to the root of your rust project, and run 'black_magic' with either the 'lambda' or 'docker' flag.

    NOTE: This project will not work if you've change the name of the build (i.e. [[bin]] name ) in your 'Cargo.toml'.
    It assumes the name of your build is the name of the folder.

    If you have a projected named 'my_project':
        - Running in lambda mode will produce a 'my_project.zip' file in 'target/black_magic'.
        - Running in docker mode will produce a 'bm_my_project' image containing the executable only, which you can then use in your own container. E.g:
            FROM bm_my_project
            EXPOSE 80/tcp
            CMD ["/my_project"]

    This project wouldn't work without this excellent project:
    https://gitlab.com/rust_musl_docker/image
    Black magic simply makes it easier to use.
    If your project doesn't compile, you may need to edit black_magic's source to select a newer tag (Current using 2020-04-23).

    Example usage:
        black_magic --lambda
        black_magic --docker
"#;

const BM_DOCKERFILE: &str = r#"
FROM registry.gitlab.com/rust_musl_docker/image:nightly-2020-04-23
RUN apt-get update
RUN apt-get install zip -y
RUN apt-get install tar -y
"#;

fn main() {
    let matches = App::new("black_magic")
        .version("1.0.0")
        .author("Peter Reeves <peter.x.reeves@gmail.com>")
        .about(USAGE)
        .arg(Arg::with_name("DOCKER")
            .help("Build a docker image.")
            .short("d")
            .long("docker"))
        .arg(Arg::with_name("LAMBDA")
            .help("Build a lambda zip.")
            .short("l")
            .long("lambda"))
        .get_matches();

    let is_docker = matches.is_present("DOCKER");
    let is_lambda = matches.is_present("LAMBDA");

    if !is_docker && !is_lambda {
        println!("You need to specify what to build. See `--help`.");
        return;
    } else if is_docker && is_lambda {
        println!("You can't specify both at once, please build one then the other.");
        return;
    }

    let docker_check = Command::new("docker").arg("--version").output().expect("Unable to test for docker. Is docker installed on your system?").stdout.starts_with(b"Docker version");
    if !docker_check {
        println!("It looks like Docker is not installed on your system. Running `docker --version` did not produce expected result.");
        return;
    }

    let current_dir = env::current_dir().expect("Unable to get current directory.");
    
    let mut cargo_toml = current_dir.to_owned();
    cargo_toml.push("Cargo.toml");
    if !cargo_toml.exists() {
        println!("This doesn't look like a rust project. Are you in the right place? No `Cargo.toml` was found in this directory.");
        return;
    }

    let mut bm_dir = current_dir.to_owned();
    bm_dir.push("target");
    bm_dir.push("black_magic");
    fs::create_dir_all(&bm_dir).expect("Unable to create `target\\black_magic` directory.");

    let image_check = Command::new("docker").arg("image").arg("inspect").arg("black_magic").output().expect("Unable to test for `black_magic` image.");
    let image_exists = std::str::from_utf8(&image_check.stderr).expect("Unable to check existing docker images");
    if image_exists.starts_with("Error: No such image: black_magic") {
        println!("Building black_magic image...");

        let mut bm_dockerfile = bm_dir.to_owned();
        bm_dockerfile.push("bm_dockerfile");
        
        fs::create_dir_all(&bm_dockerfile).expect("Unable to create `target\\black_magic\\bm_dockerfile`.");
        env::set_current_dir(&bm_dockerfile).expect("Unable to change the current dir.");

        bm_dockerfile.push("Dockerfile");

        fs::write(&bm_dockerfile, BM_DOCKERFILE).expect("Unable to create Dockerfile.");

        let image_build = Command::new("docker").arg("build").arg("-t").arg("black_magic").arg(".").output().expect("Unable to build `black_magic` image.");
        if !image_build.status.success() {
            panic!("Unable to build `black_magic` image.");
        }

        env::set_current_dir(&current_dir).expect("Unable to reset current directory.");
    }

    let project_name = current_dir.file_name().expect("Unable to get project name.").to_str().expect("Unable to get project name as string.");

    let current_dir_volume = format!("{}:/workdir", current_dir.to_str().expect("Unable to get current directory as string.").replace(r"\", r"/"));

    let cargo_home = home::cargo_home().expect("Unable to get cargo home.");

    let git_volume = {
        let mut git = cargo_home.to_owned();
        git.push("git");
        if git.exists() {
            let escaped = git.to_str().expect("Unable to get git directory as string.");
            Some(format!("{}:/root/.cargo/git", escaped.replace(r"\", r"/")))
        } else {
            None
        }
    };

    let registry_volume = {
        let mut registry = cargo_home.to_owned();
        registry.push("registry");
        if registry.exists() {
            let escaped = registry.to_str().expect("Unable to get registry directory as string.");
            Some(format!("{}:/root/.cargo/registry", escaped.replace(r"\", r"/")))
        } else {
            None
        }
    };

    /*
    Compile using `rust_musl_docker`:
        - interactive
        - remove when container finishes
        - current working directory as volume
    */
    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("-i")
        .arg("--rm")
        .arg("-v")
        .arg(current_dir_volume);

    if let Some(g) = git_volume {
        cmd.arg("-v").arg(g);
    }

    if let Some(r) = registry_volume {
        cmd.arg("-v").arg(r);
    }

    if is_docker {
        println!("Compiling project...");

        /*
        Build with:
            - release mode
            - verbose error messages
            - target musl
            - output the executable to root
        Tar:
            - executable at root
            - to output directory
            - create
            - gzip
            - With filename
        */
        let cargo_cmd = format!(
            "cargo build --release -vv --target=x86_64-unknown-linux-musl -Z unstable-options --out-dir=/ && tar -czf target/black_magic/{}.tar.gz /{}",
            project_name, project_name);

        cmd.arg("black_magic")
            .arg("/bin/bash")
            .arg("-c")
            .arg(&cargo_cmd);
        let built = cmd.output().expect("Unable to run build command.");
        if built.status.success() {
            let mut project_dockerfile = bm_dir.to_owned();
            project_dockerfile.push("Dockerfile");
            println!("{}", project_dockerfile.to_str().unwrap());
            fs::write(&project_dockerfile, format!(r#"
FROM scratch
ADD {}.tar.gz /
"#,
                project_name)).expect("Unable to create project dockerfile.");

            println!("Building project image...");

            env::set_current_dir(&bm_dir).expect("Unable to change the current dir.");

            /*
            Build project image:
                - no cache
                - tag as `bm_{project_name}`
                - using the dockerfile in the current dir
            */
            let project_image = Command::new("docker")
                .arg("build")
                .arg("--no-cache")
                .arg("-t")
                .arg(format!("bm_{}", project_name))
                .arg(".").output().expect("Unable to build project image.");

            env::set_current_dir(&current_dir).expect("Unable to reset current directory.");

            if project_image.status.success() {
                println!("Project image: bm_{}", project_name);
                println!("...Done!");
            } else {
                println!("Project image failed");
                println!("stdout: {}", std::str::from_utf8(&project_image.stdout).unwrap());
                println!("stderr: {}", std::str::from_utf8(&project_image.stderr).unwrap());
            }
        } else {
            println!("Build failed. Run the following command manually to see the problem:");
            println!();
            println!("{:?}", cmd);
            println!();
            println!("stdout: {}", std::str::from_utf8(&built.stdout).unwrap());
            println!("stderr: {}", std::str::from_utf8(&built.stderr).unwrap());
        }
    } else {
        println!("Compiling project to lambda zip...");

        /*
        Build with:
            - release mode
            - verbose error messages
            - target musl
            - output the executable to root
        Rename:
            - project name
            - "bootstrap"
        Zip:
            - no directories, just files
            - to output directory
            - from "bootstrap" at root
        */
        let cargo_cmd = format!(
            "cargo build --release -vv --target=x86_64-unknown-linux-musl -Z unstable-options --out-dir=/ && mv /{} /bootstrap && zip -j target/black_magic/{}.zip /bootstrap",
            project_name, project_name);

        cmd.arg("black_magic")
            .arg("/bin/bash")
            .arg("-c")
            .arg(&cargo_cmd);
        let built = cmd.output().expect("Unable to run build command.");
        if built.status.success() {
            println!("...Done!");
        } else {
            println!("Build failed. Run the following command manually to see the problem:");
            println!();
            println!("{:?}", cmd);
            println!();
            println!("stdout: {}", std::str::from_utf8(&built.stdout).unwrap());
            println!("stderr: {}", std::str::from_utf8(&built.stderr).unwrap());
        }
    }
}
