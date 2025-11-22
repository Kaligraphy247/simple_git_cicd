# simple_git_cicd

A lightweight, configurable Git webhook CI/CD runner designed for individual developers and small self-hosted projects.

---

## Table of Contents

- [Motivation](#motivation)
- [Features](#features)
- [How to Run](#how-to-run)
  - [Configuration (TOML)](#configuration-toml)
  - [Sample Config](#sample-config)
  - [Running the Server](#running-the-server)
- [Web UI](#web-ui)
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

## Features

- **Single Binary** - Everything bundled into one ~6MB executable (API server + Web UI)
- **Web Dashboard** - Real-time job monitoring with SSE updates
- **SQLite Storage** - Persistent job history with no external database required
- **Per-Project Config** - Different scripts per project and branch
- **Webhook Security** - HMAC signature validation for GitHub webhooks
- **Rate Limiting** - Per-project throttling with configurable request counts and time windows
- **Hot Reload** - Reload configuration without restarting the server
- **Dry Run Mode** - Test webhooks without executing scripts

---

## How to Run

### 1. Build Your Config

All projects/scripts are defined in a single `cicd_config.toml` in the root or specified by `CICD_CONFIG` env var.

#### Configuration (TOML)

Each project specifies:

**Required:**
- `name` - Repository name (matches `repository.name` from GitHub payload)
- `repo_path` - Absolute path to the project folder
- `branches` - List of branch names to trigger jobs (e.g., `["main", "staging"]`)
- `run_script` - Default script to run (can be bash, python, node, etc.)

**Optional:**
- `branch_scripts` - Table mapping branch names to specific scripts
- `with_webhook_secret` - Enable HMAC signature validation (default: false)
- `webhook_secret` - Secret for GitHub webhook validation
- `reset_to_remote` - Hard reset to remote branch before running (default: true)
- `rate_limit_requests` - Maximum number of webhook requests allowed per project within the window (default: 60)
- `rate_limit_window_seconds` - Sliding window duration for rate limiting in seconds (default: 60)

If you omit both rate limit fields, each project automatically allows up to 60 webhook requests per 60-second window.

**Lifecycle Hooks:**
- `pre_script` - Run before main script
- `post_success_script` - Run after main script succeeds
- `post_failure_script` - Run after main script fails
- `post_always_script` - Always run after main script (success or failure)

Hooks receive `CICD_MAIN_SCRIPT_EXIT_CODE` environment variable.

#### Sample Config

```toml
[[project]]
name = "my-app"
repo_path = "/home/user/code/my-app"
branches = ["main", "staging", "dev"]
run_script = "./deploy.sh"
with_webhook_secret = true
webhook_secret = "your-github-webhook-secret"
rate_limit_requests = 30                # Allow 30 webhook hits (defaults to 60 if omitted)
rate_limit_window_seconds = 60          # ...per 60-second window (defaults to 60 seconds if omitted)

# Lifecycle hooks
pre_script = "echo 'Starting deployment...'"
post_success_script = "./notify-slack.sh success"
post_failure_script = "./notify-slack.sh failure"
post_always_script = "./cleanup.sh"

[project.branch_scripts]
main = "./deploy-prod.sh"
staging = "./deploy-staging.sh"
# dev branch uses run_script as fallback

[[project]]
name = "static-site"
repo_path = "/srv/www/static-site"
branches = ["main"]
run_script = "python3 build.py"
with_webhook_secret = false
```

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

## Web UI

The server includes a built-in web dashboard accessible at the root URL (e.g., `http://localhost:8888/`).

### Dashboard Features

- **Dashboard** - Server stats, success rate, and recent jobs with real-time updates
- **Jobs List** - Browse all jobs with pagination
- **Job Details** - Timeline view, console output, commit info, and raw JSON
- **Projects** - Overview of configured projects with job stats
- **Config** - View current configuration and reload without restart

### Screenshots

- The UI supports both light and dark themes and is fully responsive for mobile devices.
(Screenshots coming soon)
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

### `POST /webhook` - GitHub Webhook

This is the endpoint you configure in GitHub webhook settings. The server validates the event, matches the project and branch, and executes the configured script.

#### Dry Run Mode

Test your webhook configuration without actually executing any scripts. The server will:
- Validate the webhook payload and signature
- Check rate limits
- Create a job record in the database
- Show what scripts *would* run in the Timeline view
- Skip all actual git operations and script execution

**Usage:**

```bash
# Using query parameter
curl -X POST "http://localhost:8888/webhook?dry_run=true" \
  -H "X-GitHub-Event: push" \
  -H "Content-Type: application/json" \
  -d @test-payload.json

# Using header
curl -X POST http://localhost:8888/webhook \
  -H "X-Dry-Run: true" \
  -H "X-GitHub-Event: push" \
  -H "Content-Type: application/json" \
  -d @test-payload.json
```

Dry run jobs are marked with a "DRY RUN" badge in the UI and can be filtered using `?dry_run=true` or `?dry_run=false` on the `/api/jobs` endpoint. Success rate calculations exclude dry run jobs.

### `GET /api/status` - Server Status

Get server information and recent jobs with optional filtering:

```bash
# Get recent jobs
curl http://localhost:8888/api/status

# Filter by project
curl "http://localhost:8888/api/status?project=myapp"

# Filter by status
curl "http://localhost:8888/api/status?status=failed"
```

### `GET /api/stats` - Server Statistics

Get server and job statistics:

```bash
curl http://localhost:8888/api/stats
```

### `GET /api/jobs` - List Jobs

Get paginated job listing with filters:

```bash
curl "http://localhost:8888/api/jobs?limit=20&offset=0"
curl "http://localhost:8888/api/jobs?project=myapp&status=success"
curl "http://localhost:8888/api/jobs?dry_run=false"  # Exclude dry runs
curl "http://localhost:8888/api/jobs?dry_run=true"   # Only dry runs
```

### `GET /api/jobs/{id}` - Job Details

Get details for a specific job by UUID:

```bash
curl http://localhost:8888/api/jobs/01234567-89ab-cdef-0123-456789abcdef
```

### `GET /api/jobs/{id}/logs` - Job Logs

Get execution logs for a specific job:

```bash
curl http://localhost:8888/api/jobs/01234567-89ab-cdef-0123-456789abcdef/logs
```

### `GET /api/projects` - List Projects

Get all configured projects with job statistics:

```bash
curl http://localhost:8888/api/projects
```

### `GET /api/config/current` - Current Configuration

Get the current TOML configuration:

```bash
curl http://localhost:8888/api/config/current
```

### `POST /api/reload` - Reload Configuration

Reload the configuration file without restarting the server:

```bash
curl -X POST http://localhost:8888/api/reload
```

### `GET /api/stream/jobs` - SSE Job Stream

Server-Sent Events stream for real-time job updates:

```bash
curl http://localhost:8888/api/stream/jobs
```

---

## How to Compile

### Prerequisites

- **Rust** (with Cargo): https://www.rust-lang.org/tools/install
- **Bun** (for UI): https://bun.sh/

### Quick Build (Native/Dev)

```sh
git clone https://github.com/kaligraphy247/simple_git_cicd.git
cd simple_git_cicd
./scripts/build_native.sh
```

This script installs UI deps via Bun, builds the SPA, and then compiles the Rust binary with the assets embedded. It assumes Rust, Bun, and other native dependencies are available on your host.

### Manual Build

1. **Build the UI:**
   ```sh
   cd ui
   bun install
   bun run build
   cd ..
   ```

2. **Build the Rust binary:**
   ```sh
   cargo build --release
   ```

3. **Set up your `cicd_config.toml`.**

4. **Run the server!**
   ```sh
   ./target/release/simple_git_cicd
   ```

### Environment Variables

- `CICD_CONFIG` - Path to config file (default: `cicd_config.toml`)
- `BIND_ADDRESS` - Server address and port (default: `127.0.0.1:8888`)
- `DATABASE_PATH` - SQLite database path (default: `cicd_data.db`)
- `RUST_LOG` - Log level filter (default: `simple_git_cicd=info` in release, `simple_git_cicd=debug` in debug builds)

**Logging examples:**
```bash
RUST_LOG=debug ./target/release/simple_git_cicd
RUST_LOG=simple_git_cicd=trace ./target/release/simple_git_cicd
RUST_LOG=simple_git_cicd=debug,tower_http=debug ./target/release/simple_git_cicd
```

### Docker Build (Cross-Platform Binary)

Need a Linux binary with a specific glibc target (e.g., `linux/amd64`) without installing Rust/Bun locally?  
The Dockerfile now builds both the UI (via Bun) and the Rust binary, then exposes the artifact so it can be copied directly into your current directory.

```bash
# Export the binary for linux/amd64 into the current directory
docker build \
  --platform linux/amd64 \
  --target artifact \
  --output type=local,dest=. \
  .
mv simple_git_cicd simple_git_cicd-linux-amd64
```

The helper script below automates those steps and names the artifact for you:

```bash
PLATFORM=linux/amd64 ./scripts/docker_build_binary.sh
# -> produces ./simple_git_cicd-linux-amd64
```

Feel free to swap `PLATFORM` (e.g., `linux/arm64`). The Dockerfile no longer hardcodes a platform, so the same file works for every target that Debian supports. The exported file is a ready-to-run binary with the UI already embedded; move it wherever you deploy your runner.

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
