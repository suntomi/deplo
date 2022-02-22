### overwrite env
- environment variable to overwrite default deplo detection of runtime configuration
  - starts with DEPLO_OVERWRITE_

### process env
- environment variable that can be used entire deplo process invocation
  - start with DEPLO_CI_ are available all execution of deplo, include local execution
  - start with DEPLO_GHACTION_ are only available when runs on github action.
  - start with DEPLO_CIRCLECI_ are only available when runs on circle ci.

### job env
- environment variable that can be used the job which is invoked as subprocess of main deplo process. 
- so don't use the code of main deplo process
  - starts with DEPLO_JOB_

### output
- output is variables that represent values which passes between jobs
- some of this does not have corresponding environment variable
  - but if exists, it starts with DEPLO_OUTPUT_


### current list of process env
- DEPLO_CI_ID
- DEPLO_CI_TAG_NAME
- DEPLO_CI_BRANCH_NAME
- DEPLO_CI_TYPE
- DEPLO_CI_CURRENT_SHA
- DEPLO_CI_RELEASE_TARGET
- DEPLO_CI_WORKFLOW_TYPE
- DEPLO_CI_PULL_REQUEST_URL
- DEPLO_CI_CLI_COMMIT_HASH
- DEPLO_CI_CLI_VERSION

### current list of ghaction specific process env
- DEPLO_GHACTION_EVENT_TYPE
- DEPLO_GHACTION_EVENT_PAYLOAD

### current list of circleci specific process env 
none

### current overwrite env
- DEPLO_OVERWRITE_COMMIT
- DEPLO_OVERWRITE_RELASE_TARGET
- DEPLO_OVERWRITE_VERBOSITY
- DEPLO_OVERWRITE_WORKFLOW

### list of job env
- DEPLO_JOB_CURRENT_NAME
- DEPLO_JOB_OUTPUT_(SYSTEM|USER)_$JOB_NAME
