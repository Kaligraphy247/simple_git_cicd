# simple_git_cicd

A lightweight, configurable Git webhook CI/CD runner designed for individual developers and small self-hosted projects.

---

## Table of Contents

- [Motivation](#motivation)
- [How to Run](#how-to-run)
  - [Configuration (TOML)](#configuration-toml)
  - [Sample Config](#sample-config)
  - [Running the Server](#running-the-server)
- [Architecture](#architecture)
- [Use Cases](#use-cases)
- [How to Compile](#how-to-compile)

---

## Motivation

Most lightweight or hobby projects just need something to pull code and redeploy when a git push happens—**without a heavy, full-blown build server, or giving up control of your box**.

**simple_git_cicd** exists for people who:

- Want automated deployment/build on push from GitHub (only GitHub is supported for now), via webhook
- Don’t want to run a full build server (Jenkins, GitLab CI, Gitea CI, etc.)
- Need scripts to be self-configurable for each repo/project: write your build in whatever language you want (Bash, Python, Node.js, etc.)
- Run on a single-user server—no multi-tenant/enterprise needs
- Want something that only acts within clearly-defined folders, and *does not take over the server or run jobs outside your config*

**If you want cloud scaling, multi-tenancy, secrets management and complex pipelines: this is NOT for you.**
If you want "git push triggers my custom script" on my cheap/affordable VPS or home machine: this is perfect.

---

## How to Run

### 1. Build Your Config

All projects/scripts are defined in a single `cicd_config.toml` in the root or specified by `CICD_CONFIG` env var.

#### Configuration (TOML)

Each project specifies:
- `name`: The repository/project name (matches `repository.name` from GitHub payload)
- `repo_path`: Absolute path to the project folder (where git & your script will run)
- `branches`: List of branch names (e.g., `[ "main", "staging" ]`) which should trigger jobs
- `run_script`: The default script/command to run for this project, if no branch-specific override is present. Can be Bash, Python, Node.js, or anything, with args.
- `branch_scripts` (optional): A table mapping branch names to script commands. If a branch matches and a script is set here, that script is used instead of `run_script`.
- `with_webhook_secret`: If true, validates the webhook with your secret.
- `webhook_secret`: (Required if above is true) The secret for GitHub HMAC signature validation.

#### Sample Config

```toml
[[project]]
name = "my-app"
repo_path = "/home/your_username/code/my-app"
branches = ["main", "staging", "dev"]
run_script = "./deploy.sh"  # original & fallback script

[project.branch_scripts]
main = "./deploy-main.sh"
staging = "./deploy-staging.sh"
# (if you trigger for 'dev', it will use run_script as fallback)

with_webhook_secret = true
webhook_secret = "yoursecret"

[[project]]
name = "static-site"
repo_path = "/srv/www/static-site"
branches = ["main"]
run_script = "python3 build_and_reload.py"
with_webhook_secret = false
# No [project.branch_scripts], all branches use run_script
```

- If `with_webhook_secret = true`, the corresponding `webhook_secret` must be present and match what GitHub's webhook uses.
- `run_script` can be any executable available on your system: a shell script, Python script, Node.js app, etc.
- `branch_scripts` (optional) lets you use a different script per branch. If no override is found for a given branch, `run_script` is used.

---

### 2. Running the Server

**1. Compile (see [How to Compile](#how-to-compile) below).**

**2. Ensure your config and scripts are ready.**

**3. Launch the server:**
```sh
./target/release/simple_git_cicd
```
By default, it reads `cicd_config.toml` in the same folder. You can specify a different config using:
```sh
CICD_CONFIG=/path/to/your_config.toml ./target/release/simple_git_cicd
```

#### Bind Address and Port

You can also control the bind address and port of the server (where it listens for webhooks) using the `BIND_ADDRESS` environment variable. By default, it listens on `127.0.0.1:8888`.

**Examples:**
- Listen on all interfaces, port 8888 (default):
  ```
  BIND_ADDRESS=0.0.0.0:8888
  ```
- Listen only on localhost, port 9001:
  ```
  BIND_ADDRESS=127.0.0.1:9001
  ```
- Listen on all interfaces, port 80:
  ```
  BIND_ADDRESS=0.0.0.0:80
  ```

**Running with a custom bind address:**
```sh
BIND_ADDRESS=127.0.0.1:9001 ./target/release/simple_git_cicd
```

You can set up your GitHub webhook to POST to the matching address, for example: `http://your-server:8888/webhook` or your chosen port.
If you running on port 80/443, then it would be `http(s)://your-server.com/webhook`.

---

## Architecture

- **Axum Web Server:** Accepts webhooks (`/webhook`), listens on a configurable port.
- **Config-driven:** All behavior (what repos, what branches, what scripts) is defined in a TOML config.
- **Project & Branch Matching:** For each webhook request, matches the repo and branch against configured projects.
- **Webhook Security:** Per-project, opt-in HMAC secret validation.
- **Script Runner:** Pulls the latest code (`git fetch`, `git switch`, `git pull`), then runs your defined script—no matter what language/tool.
- **Locking:** Ensures only one job runs at a time (mutex/lock), protecting low-resource servers from overload.

---

## Use Cases

- **Docker Rebuilds:**
  Set your `run_script` as something like:
  ```
  ./rebuild_and_restart.sh
  ```
  and in that shell script, do anything you want:
  ```sh
  #!/bin/bash
  docker-compose build
  docker-compose up -d
  ```

- **Per-branch scripts with fallback:**
  You can use the `branch_scripts` table to use a special script for `main`, and default to a simpler or different script for all others (using `run_script`).

  ```toml
  [project.branch_scripts]
  main = "./deploy-production.sh"
  develop = "./dev-deploy.sh"
  # No "feature" override, so uses run_script
  run_script = "./deploy-default.sh"
  ```
  Or simply just use `run_script` for everything if you don't need branch-specific behavior.

- **Custom Bash, Python, Node, Java, Rust, etc.:**
  ```
  run_script = "bash special-deploy.sh"
  run_script = "python3 build.py"
  run_script = "node deploy.js"
  run_script = "cargo build --release"
  ```
  Any language, as long as it is executable!

- **Absolute or Relative Scripts:**
  If you provide an absolute path, that's what is run, with the git repo folder as working directory.

---

## How to Compile

1. **Install Rust (with Cargo):**
   https://www.rust-lang.org/tools/install

2. **Clone this repo and install dependencies:**
   ```sh
   git clone https://github.com/kaligraphy247/simple_git_cicd.git
   cd simple_git_cicd
   cargo build --release
   ```

3. **Set up your `cicd_config.toml`.**

4. **Run the server!**
   ```sh
   ./target/release/simple_git_cicd
   ```


> Alternatively, build with docker if you need an older version of glibc

To compile using Docker, use the Dockerfile in the repo.
```bash
# assuming you already cloned the repo
docker build -t rust-build-buster .
```

You may want to compile inside the container, instead of at image build
```bash
docker run --rm -v "$PWD":/usr/src/myapp -w /usr/src/myapp rust-build-buster cargo build --release
```
---

## Troubleshooting

- If your script doesn’t run: check logs for permissions, paths.
- If you see "No matching project for repo ..." check your project config matches the webhook's payload fields.
- You can start minimal and expand with more projects/scripts as you go!

---

## Contributions & License
Built for simplicity and self-hosters!

MIT License

> Parts of this code and 98% of the documentation were written with LLMs
---



**Happy hacking 🚀**
