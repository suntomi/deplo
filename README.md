## Deplo
deplo is set of command line tool that aims to standardize CI/CD process. 
we aim to provide environment of "write once, run anywhere" for CI/CD. 
that is, if you build your CI/CD workflow with deplo, you can run it not only in any major CI/CD service also on localhost.
you can use multiple CI/CD at the same time too. also you have the ability to run workflow with more fine-grained control.
for instance, you can run specific workflow only when some part of your repository changed. 
this is extremely useful for running workflows on monorepo (and we love modular monolith approach for building service with microservice architecture :D)


### Glossary
- release environment
A name given to a group of resources (server infrastructures, binary download paths, etc) that are prepared to provide different revisions of the software that you are developing. eg. dev/staging/production

- release target
a branch or tag which is related with some `release environment`. for example, if your project updates `release environment` called `dev` when branch `main` is updated, we call `main` is `release target` for `release environment` `dev`.

- development branch
branches that each developper add actual commits as the output of their daily development. it should merge into one of the `release target` by creating pull request for it.

- changeset
actual changes for the repository that `release target` or `development branch` made. deplo detect paths of changeset for filtering which `jobs` need to run for this change. see below `jobs` section for example.

- jobs
shell scripts that runs when `release target` or `development branch` is created or updated. jobs are grouped into `deploy jobs` and `integrate jobs` according to whether they are invoked when a `release target` or a `development branch` is updated, and filtered with changeset contents. for example, in this repository, `deploy.builder` job will run only when `tools/docker/Dockerfile.builder` is updated for one of the `release target`.

- deploy workflow
set of jobs which runs when `release target` is updated. usually we also call the workflow as `CD(Continuous Delivery)`.

- integrate workflow
set of jobs which runs when `development branch` is created or updated. same as deploy workflow, usually we also call the workflow as `CI(Continuous Integration)`.


### Install deplo

``` bash
# macos/linux
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-$(uname -s) -o /usr/local/bin/deplo
chmod +x /usr/local/bin/deplo
# windows (on bash.exe)
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-Windows.exe -o /usr/bin/deplo
chmod +x /usr/bin/deplo
```


### setup
1. Create Deplo.toml
2. Create .env
3. Run deplo init

#### Create Deplo.toml
- example https://github.com/suntomi/deplo/blob/main/Deplo.toml

#### Create .env
deplo supports .env file to inject sensitive values as environment variable. we strongly recommend to use it, instead of writing secrets directly in Deplo.toml. when runs on localhost, deplo automatically search .env (or use `-e` option to specify path) and use it for injecting secret. for running on CI service, deplo uploads .env contents as each CI service's secrets when `deplo init` or `deplo ci setenv` is invoked.

#### Run deplo init
- if you install deplo, its really simple:
  ``` bash
  deplo init
  # this will create Deplo.toml, .circleci configuration, .github/workflows configuration, and ./deplow
  ```

- if you don't get deplo installed on your machine, use docker image for first run.
  ``` bash
  docker run --rm -ti -v $(pwd):/workdir -w /workdir ghcr.io/suntomi/deplo:latest init
  ```
- then script called `deplow` will be created on root of repository, you can use `deplow` to invoke deplo
without being deplo global installed.
  ``` bash
  ./deplow info version
  # should prints its version number
  ```


### Running deplo

``` bash
deplo ci kick # run workflow according to current branch HEAD status. with using Deplo.toml of current directory
deplo ci deploy job1 # run deploy job which name is `job1` in Deplo.toml
deplo ci integrate job2 # run integrate job which name is `job2` in Deplo.toml
```


### Roadmap
[see here](https://github.com/suntomi/deplo/issues/12)