## Deplo
Japanese version is [here](https://github.com/suntomi/deplo/blob/main/docs/README.ja.md)

Deplo is command line tool that aims to provide brand-new, unified CI/CD developemnt experience for any CI/CD services.

Deplo runs both on your local environment as command line tool to configure your CI/CD workflows, and on CI/CD services as job executer to add rich functionality to your actual job, like auto pushing your job's artifact to repository or passing dynamically generated key-value pair to other jobs.



### Motivation
Recent days, CI/CD are essential for developing and shiping production grade softwares and services. Improvement of CI/CD greatly improves development process of recent softwares and services. but compared with it, development of CI/CD itself experienced very small improvement.

Deplo gives it a shot for current development method of CI/CD itself, by solving following 3 problems.
1. Iteration
2. Configuration
3. Reusability

#### Iteration
Iteration is still largest problem of CI/CD development because of its time consuming nature. even at 2022, we still have to push dummy fix to test your new deployment workflow and (often) need to wait so long time because many dependent jobs need to run before your job starts, again and again.

through Deplo already has builtin support for filtering jobs that be going to run by which files are changed by target commit, that is not enough for reducing iteration duration.

we solve this problem by allowing to run single CI/CD job separately in your local environment, which is configured by deplo as if it runs on target CI/CD service. it seems to be similar approach that [Bitrise CLI](https://app.bitrise.io/cli) does, but deplo is not locked in any single CI/CD service.

of course I have to admit that deplo's local emuration is not 100% complete yet, to test such small difference between local environment and actual CI/CD service, deplo allows you to run your single CI/CD job on target CI/CD service separately (we call it remote job execution)

Deplo also provides ssh session for CI/CD service to "investigate immediately after failure". unlike circle CI's built in feature, you make Deplo to create ssh session right after CI/CD job failed, with only single line of configuration.

These features are drastically reduce the try-and-error iteration duration for CI/CD development. 

bonus, running any CI/CD jobs in project from your local environment helps you so much, when emerged (and often adhoc) trouble shooting of your service required. because in such a situation, using normal release flow (create PR/review/merge/make tag/etc...) to fix problem, is likely to be waste of time or source of another mistake.

#### Configuration
most of CI services use yaml for their configuration format. the format is handy when project remain small and simple. but for large one, it turns to be nightmare. 

Deplo uses [toml](https://github.com/alexcrichton/toml-rs), consise, modern and better structured text format for config files, with original support of multiline inline table. so no more need to write your settings by error-prune yaml format, because it is auto genrated by Deplo from its Deplo.toml.

Compare [Deplo.toml](https://github.com/suntomi/deplo/blob/main/Deplo.toml) and [.github/workflows/deplo-main.yaml](https://github.com/suntomi/deplo/blob/main/.github/workflows/deplo-main.yml) of the repository to know how its improved.

#### Reusability
most of CI service have its own module system, like actions for Github Action, orbs for Circle CI, to avoid to write routine job step (eg. checkout code from repository, send image to container repository) repeatedly, 

these modules will reduce an amount of development of your CI/CD workflow. but it has significant drawback. that is, once you start to use them, your jobs lose local testability, because these modules do not designed to run other environment than theirs service. already explained above, losing local testability significantly slows down CI/CD development iteration duration.

Deplo will (instroduced 0.3.0) provide new module system which has same functionality that circle CI orbs or github actions provides but will not be locked in each CI service and can be run locally, to archive good reusability without losing local testability, which both are essential for better CI/CD development experience.



### Glossary
- `release environment` A name given to a group of resources (server infrastructures, binary download paths, etc) that are prepared to provide different revisions of the software that you are developing. eg. dev/staging/production

- `release target` a branch or tag which is synchronized with some `release environment`. for example, if your project configured to update `release environment` called `dev` when branch `main` is updated, we call `main` is `release target` for `release environment` `dev` and all changes to `main` will be synchornized with `release environment ` `dev`.

- `development branch` branches that each developper add actual commits as the output of their daily development. it should be merged into one of the `release target` by creating pull request for it. by default, deplo only runs jobs when pull request from `development branch` to `release target` is created.

- `changeset` actual changes for the repository that `release target` or `development branch` made. deplo detect paths of changeset for filtering which `job`s need to run for this change. see below `job` section for example.

- `job` shell scripts that runs when `release target` or `development branch` is created or updated. jobs are grouped into `deploy job`s and `integrate job`s according to whether they are invoked when a `release target` or a `development branch` is updated, and filtered with changeset contents. for example, in this repository, `deploy.builder` job will run only when `tools/docker/Dockerfile.builder` is updated for one of the `release target`.

- `deploy workflow` set of jobs which runs when `release target` is updated. usually we also call the workflow as `CD(Continuous Delivery)`.
- `deploy job` job related with `deploy workflow`. defined by section like `[jobs.$name]` with `on = { workflows = ["integrate"] }` in Deplo.toml

- `integrate workflow` set of jobs which runs when `development branch` is created or updated. same as deploy workflow, usually we also call the workflow as `CI(Continuous Integration)`.
- `integrate job` job related with `integrate workflow`. defined section like `[jobs.$name]` with `on = { workflows = ["deploy"] }` in Deplo.toml



### Install Deplo
#### supported OS
- Linux
- MacOS
- Windows
  - now only confirmed on git-bash.exe

#### prerequisite tools
- curl
- git
- docker

#### install binary
``` bash
# macos/linux
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-$(uname -s) -o /usr/local/bin/deplo
chmod +x /usr/local/bin/deplo
# windows (on git-bash)
curl -L https://github.com/suntomi/deplo/releases/download/${version_to_install}/deplo-Windows.exe -o /usr/bin/deplo
chmod +x /usr/bin/deplo
```



### Configure your project CI/CD to use Deplo
#### overview
1. Create Deplo.toml
2. Create .env
3. Run deplo init

#### Create Deplo.toml
- example https://github.com/suntomi/deplo/blob/main/Deplo.toml
- please refer comment for usage detail (TODO: more detailed document required)

#### Create .env
deplo supports .env file to inject sensitive values as environment variable. we strongly recommend to use it, instead of writing secrets directly in `Deplo.toml`. when runs on localhost, deplo automatically search .env at repository root (or use `-e` option to specify path) and use it for injecting secret. for running on CI service, deplo uploads .env contents as each CI service's secrets when `deplo init` or `deplo ci setenv` is invoked.

#### Run deplo init
- if you already install deplo, its really simple:
  ``` bash
  deplo init
  # this will create Deplo.toml, .circleci configuration, .github/workflows configuration, and ./deplow
  ```

- if you don't get deplo installed on your machine, and don't want to install for now, can use docker image for first run.
  ``` bash
  docker run --rm -ti -v $(pwd):/workdir -w /workdir ghcr.io/suntomi/deplo:latest init
  ```

- then script called `deplow` will be created on root of repository, your team member can use `deplow` to invoke deplo without being deplo globally installed. to avoid version skew, we recommend to use ./deplow if it exists.
  ``` bash
  ./deplow info version
  # should prints its version code
  ```



### Running deplo jobs
there is 3 way to run Deplo jobs.

1. from CI service
2. from command line (Local)
3. from command line (Remote)


#### from CI
this is probably most familiar for you. if push or pull request is made for your `release target`, github actions or circle ci starts their workflow.
in the workflow, it runs `deplo ci kick` and `job`s defined in Deplo.toml will be executed according to the `changeset` that push or pull request contains.

you can also run `deplo ci kick` in your local environment, please don't forget to specify `-r` option like `deplo -r nightly ci kick`, if you are not on `release target branch`.

#### from command line (Local)
each job defined in Deplo.toml can be run separately by using `deplo i -r $release_target $job_name`(for `integrate job`) or `deplo d -r $release_target $job_name`(for `deploy job`).
also you can interact with the environment that executes corresponding job in various way.

log in to the shell: `deplo i -r $release_target $job_name sh`

running adhoc comman: `deplo i -r $release_target $job_name sh ${adhoc command args}`

running pre-defined command line args: `deplo i -r $release_target $job_name sh @task_anme`

you can set adhoc environment variable too: `deplo i -r $release_target $job_name -e ENV1=VAR1`

or specify commit SHA to run job: `deplo i -r $release_target $job_name --ref efc6d3e2c1a1d875517bf81fb3ac193541050398`


#### from command line (Remote)
you can run your job __on actual CI service environment__ with `--remote`
run command `${adhoc command args}` remotely on CI service environment of `$job_name`: `deplo i -r $release_target $job_name --remote sh ${adhoc command args}`


### Roadmap
[see here](https://github.com/suntomi/deplo/issues/12)
