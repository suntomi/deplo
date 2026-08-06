# Deplo.toml generation notes for coding agents

This file is an implementation-oriented schema guide for generating `Deplo.toml`.
Prefer the keys and shapes below over older examples.

Source of truth:

- top-level config: `core/src/config.rs`
- entrypoint: `config::Config::create`
- secret/var pre-load: `core/src/config/var.rs`
- job/workflow schema: `core/src/config/job.rs`, `core/src/config/workflow.rs`
- CI/VCS account schema: `core/src/config/ci.rs`, `core/src/config/vcs.rs`

## Load order and required sections

`Config::create` loads the same TOML twice:

1. As `config::var::Config`, before the main config. This expects both `secrets` and `vars`.
2. As `config::Config`, after secrets/vars are registered.

Therefore generate these sections even when empty:

```toml
[secrets]

[vars]
```

Required top-level fields for main config:

```toml
version = 1
project_name = "project-name"

[release_targets]

[vcs]

[ci.default]

[workflows]

[jobs.example]
```

Optional top-level fields:

```toml
data_dir = ".deplo"
debug = { ghaction_job_debugger = "default", ghaction_deplo_debugger = "default" }
checkout = { fetch_depth = 2, lfs = false, submodules = false, token = "${TOKEN_SECRET}" }
update_check_schedule = "17 3 * * *"
```

Effects:

- `version`: config version. Defaults to `1` if omitted, but emit it explicitly.
- `project_name`: used for naming local fallback images and project identity.
- `data_dir`: base directory for Deplo runtime data. Defaults to `<repo>/.deplo`.
- `debug`: GitHub Actions debugger generation knobs. Known keys are `ghaction_job_debugger` and `ghaction_deplo_debugger`; values `none` disables, `always` always adds runnable debugger, other values are default conditional behavior.
- `checkout`: global checkout options merged with per-job `checkout`.
- `update_check_schedule`: cron expression for generated Deplo update workflow. Default is `17 3 * * *`.

## Value references

Fields typed as `config::Value` must be TOML strings.

Supported interpolation forms:

```toml
plain = "literal value"
secret_ref = "${SECRET_OR_VAR_NAME}"
env_ref = "${env:PROCESS_ENV_NAME}"
```

Rules:

- `${NAME}` resolves from loaded `[secrets]` through `config::secret`.
- `[vars]` entries are injected into job environments, but `config::Value` reference resolution does not look them up.
- Names must match `^[A-Za-z_][A-Za-z_0-9]*$`.
- `${env:NAME}` resolves from the process environment at runtime.
- Interpolation only applies when the whole string is exactly `${...}`. Embedded text like `"prefix-${NAME}"` is not resolved by `config::Value`.

Fields typed as `config::AnyValue` may be string, number, bool, array, or table. If an `AnyValue` is a string and exactly `${...}`, the same reference rules apply.

## Secrets and vars

Use current key names `env` and `path`. Do not generate older `{ type = "env", key = "..." }` forms.

```toml
[secrets]
GITHUB_TOKEN = { env = "GITHUB_TOKEN" }
APP_PRIVATE_KEY = { path = "./.secrets/app.pem" }
AWS_ROLE = { env = "AWS_ROLE", targets = ["actions"] }

[vars]
BUILD_MODE = { env = "BUILD_MODE" }
LOCAL_VERSION = { path = "./VERSION" }
```

Shape:

```toml
NAME = { env = "ENV_NAME", targets = ["optional", "target", "names"] }
NAME = { path = "relative/or/absolute/file", targets = ["optional"] }
```

Effects:

- `env`: value is read from a process environment variable during config creation.
- `path`: locally reads a file relative to repository root. On CI, file vars/secrets are expected in an environment variable named as the secret/var key.
- `targets`: forwarded to CI secret/var syncing. For GitHub Actions, target names are passed to `set_secret`/`set_var`.

`var::Var` has a `Module` variant in code, but the factory currently panics for module vars. Do not generate module var/secret settings.

## Release targets

```toml
[release_targets]
nightly = { patterns = ["main"] }
prod = { tag = true, patterns = ["v[0-9]*", "[0-9]*"] }
```

Shape:

```toml
target_name = { patterns = ["glob-like-pattern"], tag = false }
```

Effects:

- Default `tag` is `false`, so the target matches branches.
- `tag = true` makes the target match tags.
- `patterns` are passed to VCS release target logic.
- Job triggers can restrict by `release_targets = ["target_name"]`.

## VCS account

Exactly one `[vcs]` account is configured.

GitHub PAT:

```toml
[vcs]
type = "github"
email = "${BOT_EMAIL}"
account = "${BOT_ACCOUNT}"
key = "${BOT_TOKEN}"
```

GitHub App:

```toml
[vcs]
type = "github_app"
app_id = "${APP_ID}"
pkey = "${APP_PRIVATE_KEY}"
local_fallback = {
  email = "${BOT_EMAIL}",
  account = "${BOT_ACCOUNT}",
  key = "${BOT_TOKEN}",
}
```

GitLab:

```toml
[vcs]
type = "gitlab"
email = "${BOT_EMAIL}"
account = "${BOT_ACCOUNT}"
key = "${BOT_TOKEN}"
```

Module:

```toml
[vcs]
type = "module"
uses = "owner/module@version"
with = { option = "value" }
```

Effects:

- `github`/`gitlab`: use account, email, key for repository operations.
- `github_app`: uses app id and private key; `local_fallback` supplies PAT credentials for local execution.
- `module`: uses a VCS module config (`uses`, optional `with`).

## CI accounts

Generate `[ci.default]`. Additional accounts use `[ci.<name>]`. Jobs select an account with `account = "name"`; default is `"default"`.

GitHub Actions PAT:

```toml
[ci.default]
type = "ghaction"
account = "${BOT_ACCOUNT}"
key = "${BOT_TOKEN}"
```

GitHub Actions App:

```toml
[ci.default]
type = "ghaction_app"
app_id_secret_name = "APP_ID"
pkey_secret_name = "APP_PRIVATE_KEY"
local_fallback = { account = "${BOT_ACCOUNT}", key = "${BOT_TOKEN}" }
```

CircleCI:

```toml
[ci.circle]
type = "circleci"
key = "${CIRCLECI_TOKEN}"
```

Module:

```toml
[ci.custom]
type = "module"
uses = "owner/ci-module@version"
with = { option = "value" }
```

Effects:

- `ghaction`: generates GitHub Actions workflow files and uses the PAT.
- `ghaction_app`: generated checkout uses an app token step; local execution uses `local_fallback` when present.
- `circleci`: generates `.circleci/config.yml`.
- `module`: uses a CI module config.

## Module source syntax

Where a module `uses` value is accepted, these forms are valid:

```toml
uses = "owner/module@version"
uses = { git = "https://example.com/repo.git", tag = "v1.0.0" }
uses = { git = "https://example.com/repo.git", rev = "commit-or-ref" }
uses = { url = "https://example.com/module.tar.gz" }
uses = { path = "./tools/modules/name" }
```

Always include `with = {}` only when options are needed. Module options are arbitrary TOML values.

## Workflows

`workflows.setup()` implicitly adds these reserved workflows:

- `deploy`
- `integrate`
- `Deplo System`

Do not define those names under `[workflows]`.

Cron workflow:

```toml
[workflows]
cron = {
  schedules = { daily = "30 0 * * *", weekly = "0 0 * * 0" },
  priority = 1,
}
```

Repository event workflow:

```toml
[workflows]
repository = {
  events = {
    issue_change = ["issues.opened", "issues.closed"],
    manual = ["workflow_dispatch"],
    external = ["repository_dispatch.external"],
  },
  priority = 1,
}
```

Dispatch workflow:

```toml
[workflows]
manual_dispatch = {
  manual = true,
  inputs = {
    target = { required = true, type = "string", default = "dev", description = "target env" },
    count = { type = "number", default = 1 },
    ratio = { type = "float", default = 0.5 },
    dry_run = { type = "bool", default = false },
    choice = { options = ["dev", "stage", "prod"], default = "dev" },
    list_value = { type = "list", schema = { type = "string" } },
    object_value = { schema = { name = { type = "string" }, enabled = { type = "bool" } } },
    tuple_value = { schema = [{ type = "string" }, { type = "number" }] },
  },
  priority = 1,
}
```

Module workflow:

```toml
[workflows]
slack = {
  uses = "owner/workflow-module@version",
  with = { option = "value" },
  release_target = "nightly",
  priority = 1,
  inherit_from = ["integrate"],
}
```

Effects:

- Only one cron workflow and one repository workflow may be defined.
- `priority` defaults to `1` for cron/repository/dispatch/module workflows. Built-in `deploy` and `integrate` priority is `0`.
- `inherit_from` lets a module workflow react like listed workflows for job trigger matching.
- `release_target` exists in workflow module extension data; it is module-extension metadata, not a job trigger filter.

Input schema classes:

```toml
input = { type = "number" }
input = { type = "string" }
input = { type = "bool" }
input = { type = "float" }
input = { type = "list", schema = { type = "string" } }
input = { type = "map", schema = { type = "string" } }
input = { options = ["a", "b"] }
input = { schema = [{ type = "string" }, { type = "number" }] }
input = { schema = { nested = { type = "bool" } } }
```

`required`, `description`, and scalar `default` are accepted on every input schema.

## Jobs

Minimum job:

```toml
[jobs.test]
on = { workflows = ["integrate"], changed = ["**/*.rs", "Cargo.*"] }
runner = { os = "linux" }
command = "cargo test"
```

Full job shape:

```toml
[jobs.name]
account = "default"
on = [
  { workflows = ["integrate"], changed = ["**/*.rs"], diff_matcher = "glob" },
  { workflows = ["deploy"], release_targets = ["prod"], changed = ["**/*.rs"] },
  { workflows = ["cron"], schedules = ["daily"] },
  { workflows = ["repository"], events = ["issue_change"] },
  { workflows = ["slack"], when = { message_matches = "deploy" } },
  { workflows = ["deploy"] },
]
runner = { os = "linux" }
shell = "bash"
command = "cargo test"
env = { RUST_BACKTRACE = "1" }
workdir = "."
checkout = { fetch_depth = 2, lfs = false, submodules = false }
caches = { cargo = { keys = ["cargo-${{ runner.os }}"], paths = ["target", "~/.cargo"] } }
depends = ["other-job"]
commit = [{ files = ["generated/**"], with = "push", squash = true }]
options = { debugger = "default", native_configs = { permissions = { contents = "read" } } }
tasks = { test = "cargo test", lint = "cargo clippy" }
```

Effects:

- `account`: selects a key from `[ci]`. Defaults to `default`.
- `on`: a single trigger table or an array of trigger tables.
- `runner`: controls generated GitHub Actions runner/container and local execution behavior.
- `shell`: shell for command eval.
- `command`: string command used when `steps` is omitted.
- `steps`: ordered step list. If present, runner executes steps instead of single `command`.
- `env`: merged into job environment after common/env/secret/var/CI envs.
- `workdir`: working directory for commands and CircleCI workdir.
- `checkout`: per-job checkout options, merged with top-level checkout.
- `caches`: generated as CI cache steps. First key is primary, later keys are restore keys.
- `depends`: creates generated CI dependencies and exposes upstream user output env.
- `commit`: auto-commit settings applied after job execution.
- `options`: arbitrary option map. GitHub Actions recognizes `debugger` and `native_configs`.
- `tasks`: named local commands invoked by CLI task syntax.

### Job triggers

All trigger tables may include target filters:

```toml
workflows = ["deploy", "integrate", "cron", "repository", "module-workflow-name"]
release_targets = ["prod", "nightly"]
```

Condition variants:

```toml
# changed files on deploy/integrate workflows
{ changed = ["src/**", "Cargo.*"], diff_matcher = "glob" }
{ changed = [".*/src/.*"], diff_matcher = "regex" }

# cron workflow: names from workflows.<cron>.schedules
{ workflows = ["cron"], schedules = ["daily"] }

# repository workflow: names from workflows.<repository>.events
{ workflows = ["repository"], events = ["issue_change"] }

# module workflow: condition passed to module filter_context
{ workflows = ["slack"], when = { message_matches = "deploy" } }

# no condition: matches the target workflow/release target only
{ workflows = ["deploy"] }
```

Defaults:

- `diff_matcher` defaults to `"glob"`.
- If `workflows` is absent, trigger target matching allows any workflow whose type can react to the condition.
- If `release_targets` is absent, no release target restriction is applied.
- Although the Rust enum has an `Any` condition variant, do not generate `any = true`; use a trigger table with only target filters.

### Runners

Machine runner:

```toml
runner = { os = "linux" }
runner = { os = "windows" }
runner = { os = "macos" }
runner = { os = "linux", image = "ubuntu-24.04", class = "large" }
```

Machine runner with local fallback:

```toml
runner = {
  os = "linux",
  no_fallback = false,
  local_fallback = {
    image = "docker:25.0.1",
    shell = "sh",
    inputs = ["src", "Cargo.toml"],
    caches = ["/tmp/target"],
  },
}
```

Dockerfile fallback:

```toml
runner = {
  os = "linux",
  local_fallback = {
    path = "tools/docker/Dockerfile.local",
    repo_name = "local-builder",
    args = { BASE = "ubuntu:24.04" },
    shell = "sh",
  },
}
```

Container runner:

```toml
runner = {
  image = "ghcr.io/org/image:tag",
  inputs = ["src", "Cargo.toml"],
}
```

Effects:

- Valid `os` strings are `linux`, `windows`, `macos`.
- A runner with `os` deserializes as machine runner. A runner with top-level `image` and no `os` deserializes as container runner.
- For GitHub Actions, machine defaults are `ubuntu-latest`, `windows-latest`, `macos-latest`.
- Container runner always uses Linux runtime in Deplo.
- `local_fallback` is used for local execution when the host OS differs, or when host OS matches but `no_fallback` is not `true`.
- `local_fallback.inputs` and container runner `inputs` define mounted input paths for local container execution.
- `local_fallback.caches` define mounted cache paths for local fallback containers.

Input path forms:

```toml
inputs = ["src", "Cargo.toml"]
inputs = [{ includes = ["src/**"], excludes = ["target/**"] }]
```

### Checkout

```toml
checkout = {
  lfs = true,
  submodules = true,
  fetch_depth = 0,
  token = "${GITHUB_TOKEN}",
}
```

Shape:

```toml
lfs = true | false
submodules = true | false | "recursive"
fetch_depth = 0
token = "${SECRET_NAME}"
```

Effects:

- GitHub Actions checkout emits `fetch-depth`, `lfs`, `submodules`, and `token`.
- For GitHub App CI accounts, checkout token is generated from app token step; configured `token` is ignored by generated checkout.
- `fetch_depth = 0` enables cached checkout path in GitHub Actions.
- If any committing job uses LFS checkout, generated Deplo halt job enables LFS checkout.

### Steps

Use `steps` when a job needs multiple commands or modules.

```toml
steps = [
  { name = "build", command = "cargo build", shell = "bash", workdir = "." },
  { name = "test", exec = ["cargo", "test"], workdir = "." },
  { name = "notify", uses = "owner/step-module@version", with = { channel = "#ci" } },
]
```

Each step accepts:

```toml
name = "optional-name"
env = { KEY = "value" }
```

Command variants:

- `command`: shell-evaluated command string, optional `shell`, optional `workdir`.
- `exec`: argv list, optional `workdir`.
- module step: `uses`, optional `with`.

### Cache

```toml
caches = {
  cargo = {
    keys = ["primary-key", "restore-key-1", "restore-key-2"],
    paths = ["target", "~/.cargo"],
  },
}
```

Effects:

- `keys[0]` is the primary cache key.
- `keys[1..]` become restore keys for GitHub Actions.
- `paths` are passed to CI cache action.

### Commit

Use singular `commit`, not older `commits`.

Push:

```toml
commit = { files = ["generated/**"], with = "push", squash = true }
```

Pull request:

```toml
commit = {
  files = ["generated/**"],
  on = { workflows = ["deploy"], release_targets = ["prod"] },
  with = "pull_request",
  labels = ["auto"],
  assignees = ["owner"],
  aggregate = false,
}
```

Array form:

```toml
commit = [
  { files = [".github/**"], with = "push" },
  { files = ["docs/**"], on = { release_targets = ["prod"] }, with = "pull_request", aggregate = true },
]
```

Shape:

```toml
files = ["changed file patterns to push"]
on = { workflows = ["deploy"], release_targets = ["prod"] }
log_format = "[deplo] update generated files"
with = "push" | "pull_request"
squash = true
labels = ["label"]
assignees = ["user"]
aggregate = false
```

Effects:

- After the job runs, Deplo pushes matching changed files to temporary branches.
- Cleanup/Halt aggregates those branches.
- `push.squash` defaults to `true`.
- `pull_request.aggregate` defaults to `false`.
- `log_format` controls the intermediate commit message. Default is `[deplo] update by job {job_name}`.

### Options

`options` is arbitrary `AnyValue` map. Known consumers:

```toml
options = {
  debugger = "none",
  native_configs = {
    permissions = { contents = "read", "id-token" = "write" },
    timeout-minutes = 30,
  },
}
```

Effects:

- GitHub Actions `debugger`: `none` disables job debugger, `always` makes it unconditional, other/omitted uses global/default conditional behavior.
- GitHub Actions `native_configs`: table serialized directly into generated job declaration YAML.

## Recommended skeleton

```toml
version = 1
project_name = "example"
checkout = { fetch_depth = 2 }
update_check_schedule = "17 3 * * *"

[secrets]
BOT_ACCOUNT = { env = "BOT_ACCOUNT" }
BOT_EMAIL = { env = "BOT_EMAIL" }
BOT_TOKEN = { env = "BOT_TOKEN" }

[vars]

[release_targets]
nightly = { patterns = ["main"] }
prod = { tag = true, patterns = ["v[0-9]*"] }

[vcs]
type = "github"
email = "${BOT_EMAIL}"
account = "${BOT_ACCOUNT}"
key = "${BOT_TOKEN}"

[ci.default]
type = "ghaction"
account = "${BOT_ACCOUNT}"
key = "${BOT_TOKEN}"

[workflows]
cron = { schedules = { daily = "30 0 * * *" } }

[jobs.test]
on = [
  { workflows = ["integrate"], changed = ["src/**", "Cargo.*"] },
  { workflows = ["cron"], schedules = ["daily"] },
]
runner = { os = "linux" }
command = "cargo test"
```

## Keys to avoid when generating new configs

Avoid these older or misleading forms:

- `commits = [...]`: current field is `commit`.
- `method = { type = "push" }`: current commit method tag is flattened as `with = "push"` or `with = "pull_request"`.
- secret/var entries like `{ type = "env", key = "NAME" }`: current forms are `{ env = "NAME" }` and `{ path = "file" }`.
- defining `[workflows.deploy]`, `[workflows.integrate]`, or `[workflows."Deplo System"]`: these names are reserved and inserted automatically.
