export const dummyConfig = `
# YOU SHOULDN'T BE ABLE TO SEE THIS!!! WTF are you doing???
[[project]]
name = "example-project"
repo_path = "/home/user/projects/example"
branches = ["main", "staging", "development"]
run_script = "./deploy.sh"
with_webhook_secret = true
webhook_secret = "lorem-ipsum-dolor-sit-amet-consectetur"
rate_limit_requests = 30
rate_limit_window_seconds = 60

# Lifecycle hooks
pre_script = "echo 'Starting deployment...'"
post_success_script = "./notify-success.sh"
post_failure_script = "./notify-failure.sh"
post_always_script = "./cleanup.sh"

# Branch-specific scripts
[project.branch_scripts]
main = "./deploy-production.sh"
staging = "./deploy-staging.sh"

# Branch-specific repo paths
[project.branch_repo_paths]
feature-x = "/home/user/projects/example-feature-x"

[[project]]
name = "another-project"
repo_path = "/srv/www/another-project"
branches = ["main"]
run_script = "python3 build.py"
with_webhook_secret = false`;
