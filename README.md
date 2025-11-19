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

**All customization lives in your scripts.** The runner is intentionally generic - it just handles git operations and runs your scripts. Your use cases could be:

- ✅ **Web app deployments** - Node.js, Python, PHP apps with PM2, systemd, or any process manager
- ✅ **Rust/Go binary compilation** - Build and upload to CDN, S3, or GitHub Releases
- ✅ **Docker image builds** - Build and push to container registry
- ✅ **Mobile app builds** - Compile and deploy to TestFlight, Play Store, etc.
- ✅ **Documentation generation** - Build docs and deploy to GitHub Pages, S3, etc.
- ✅ **Literally anything git-triggered** - If it can run in a shell script, it can run here

### Example Use Cases

**Docker Rebuilds:**
```toml
run_script = "./rebuild_and_restart.sh"
```
```sh
#!/bin/bash
docker-compose build
docker-compose up -d
```

**Per-branch deployment with PM2:**
```toml
[project.branch_scripts]
main = "./deploy-production.sh"    # Deploy to port 3000
staging = "./deploy-staging.sh"    # Deploy to port 3001
develop = "./deploy-develop.sh"    # Deploy to port 3002
```

**Rust binary to CDN:**
```sh
#!/bin/bash
cargo build --release
aws s3 cp target/release/myapp s3://my-cdn/downloads/myapp-latest
```

**Custom Bash, Python, Node, Java, Rust, etc.:**
```toml
run_script = "bash special-deploy.sh"
run_script = "python3 build.py"
run_script = "node deploy.js"
run_script = "cargo build --release"
```
Any language, as long as it is executable!

---

## API Endpoints

The server provides several endpoints for monitoring and management:

### `GET /` - Health Check

**Plain text response (default):**
```bash
curl http://localhost:8888/
# Returns: "simple_git_cicd - healthy"
```

**JSON response:**
```bash
curl "http://localhost:8888/?format=json"
```
```json
{
  "name": "simple_git_cicd",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "current_job": "01234567-89ab-cdef-0123-456789abcdef",
  "total_projects": 3,
  "jobs_completed": 42,
  "status": "healthy"
}
```

### `GET /status` - Job Status

Get detailed server and job information with optional filtering:

```bash
# Get recent jobs
curl http://localhost:8888/status

# Filter by project
curl "http://localhost:8888/status?project=myapp"

# Filter by status
curl "http://localhost:8888/status?status=failed"

# Filter by project and branch
curl "http://localhost:8888/status?project=myapp&branch=main"
```

### `GET /job/:id` - Individual Job Details

Get details for a specific job by UUID:

```bash
curl http://localhost:8888/job/01234567-89ab-cdef-0123-456789abcdef
```

### `POST /webhook` - GitHub Webhook

This is the endpoint you configure in GitHub webhook settings. The server validates the event, matches the project and branch, and executes the configured script.

### `POST /reload` - Reload Configuration

Reload the configuration file without restarting the server. The reload waits for any currently running job to finish before applying the new configuration.

```bash
curl -X POST http://localhost:8888/reload
```

**Response on success:**
```json
{
  "status": "success",
  "message": "Configuration reloaded successfully"
}
```

**Response on error:**
```json
{
  "status": "error",
  "message": "Failed to parse config: invalid TOML syntax at line 5"
}
```

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
