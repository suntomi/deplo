## Deplo
deplo is set of command line tool that standardized CI/CD process. 
we aim to provide environment of "write once, run anywhere" for CI/CD. 
that is, if you build your CI/CD pipeline with deplo, you can run the pipeline not only in any major CI/CD service also on localhost.
you can use multiple CI/CD at the same time too. also you have the ability to run workflow with more fine-grained control.
for instance, you can run specific workflow only when some part of your repository changed. 
this is extremely useful for running workflows on monorepo (and we love modular monolith approach for building service with microservice architecture :D)


### Init project for deplo

``` bash
deplo init # will create Deplo.toml, .circleci configuration, .github/workflows configuration.
```


### Glossary
- release environment
A name given to a group of infrastructures that are prepared to run different revisions of the software you are developing. eg. dev/staging/production

- release target
branch name which is related with some CD target. for example, if your project always update release environment `dev` when branch `main` is updated, we call `main` is `release target branch` for environment `dev`.

- development branch
branches that each developper add actual commits as the output of their daily development.

- changeset
actual changes for the repository that development branch made. deplo uses file names of changeset as filtering which pipelines need to run.

- deploy
set of pipelines which runs when `release target branch` is updated. usually we understand these pipelines as `CD(Continuous Delivery) pipeline`.

- integrate
set of pipelines which runs when `development branch` is created or updated. usually we understand these pipelines as `CI(Continuous Integration) pipeline`.


### Edit Deplo.toml
- example

``` toml
[common]
project_name = "deplo"
data_dir = "deplo"

[common.release_targets]
# key value pair of `release environment` = `release target branch`
dev = "main"
prod = "release"

[vcs]
# account information of version control system
type = "Github"
email = "${DEPLO_VCS_ACCESS_EMAIL}"
account = "suntomi-bot"
key = "${DEPLO_VCS_ACCESS_KEY}"

# you can use multiple ci service with [ci.$account_name]
[ci.account.default]
type = "GhAction"
account = "suntomi-bot"
key = "${DEPLO_VCS_ACCESS_KEY}"

# non-default CI setting example
[ci.account.circleci]
type = "CircleCI"
key = "${DEPLO_CI_ACCESS_KEY}"

# ====== continuous integration ======
# `integrate` contains key value pair of `job name` = `{account, patterns, machine, container, command, cache, workdir, depends_on}`
# changeset is detected as following rule
# if the branch has related pull/merge request, git diff ${base branch}...${head branch} is used.
# if the branch does not have any pull/merge request, deplo try to find nearest ancestor branch with the same manner as
# https://stackoverflow.com/a/17843908/1982282, and use it as base branch.
[ci.workflow.integrate.data]
# account to be used to run job, if omitted, default account will be used
account = circleci

# regexp of file name pattern appeared in changeset. any of regexp matched then this pipeline will be invoked
patterns = ["data/.*"]

# machine type
machine = "ubuntu-latest"

# workdir. if omitted, deplo will use container/machine default
workdir = "/tmp/workdir"

# invoking command for CI
command = """
bash ./tools/data/build.sh
""" 

# dependent job
depends_on = ["integrate-client"]

# cache setting. multiple cache can be set. execution order is: 
# restore: array appearing order
# save: reverse array appearing order
[[ci.workflow.integrate.data.cache]]
# keys for using find cache entry. some directive like {{ .Branch }} can be used but because it is CI service specific,
# consulting each CI service document for detail. 
# (I hope each CI service provider offers cache feature with cli, then this can be more standardized)
restore_keys = ["source-v1-{{ .Branch }}-{{ .Revision }}", "source-v1-{{ .Branch }}-", "source-v1-"]
save_key = "source-v1-{{ .Branch }}-{{ .Revision }}"
path = "data/built"


# ====== continuous deployment ======
# deploy is also key value pair of `job name` = `Job(same object as for integrate config)`
# for deploy, changeset is detected by git diff HEAD^
[ci.workflow.deploy.data]
patterns = ["data/.*"]
container = "suntomi/aws-cli"
command = """
bash ./tools/data/upload.sh
"""
[[ci.workflow.deploy.data.cache]]
restore_keys = ["source-v1-{{ .Branch }}-{{ .Revision }}", "source-v1-{{ .Branch }}-", "source-v1-"]
# omitting save_key or path refrains from saving cache

[ci.workflow.integrate.client]
patterns = ["client/.*"]
command = """
bash ./tools/client/run-test.sh
"""

[ci.workflow.deploy.client]
patterns = ["client/.*"]
command = """
bash ./tools/client/build-and-upload.sh
"""
```


### Secrets
deplo supports .env file to inject sensitive values as environment variable. when run on localhost, .env is present in repository
and deplo automatically load and use values. you can upload .env contents as CI service's secret by using `deplo ci setenv`.


### Running deplo

``` bash
deplo ci kick # run pipeline according to current branch HEAD status. uses Deplo.toml of current directory
deplo ci kick "data/.*" # run workflow that related with specific changeset
```
