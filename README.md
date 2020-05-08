### commands
- ```deplo init```: init deplo project structure
    - mkdir config.common.data_dir
    - 
- ```deplo exec```: execute 3rd party command (aws, aliyun, gcloud, terraform, etc.)
- ```deplo ci```: run ci with configuration
- ```deplo service```: service related subcommand
    - ```deplo service create```: create service
    - ```deplo service deploy```: run service
- ```deplo infra```: control infrastructure
    - ```deplo infra plan```: generate plan
    - ```deplo infra apply```: apply generated plan
    - ```deplo infra eval```: evaluate value of infrastructure  

### deplo datadir structure
```
- deplo
    +- infra 
        +- modules          # terraform modules
    +- services             # service scripts
    +- versions             # metadata json
    +- resources            # data files like mobileprovision of iOS app
```

### deplo.toml example
``` toml
[common]
deplo_image = "suntomi/deplo",
data_dir = "meta"
no_confirm_for_prod_deploy = false

[cloud.provider.GCP]
key = "$DEPLO_CLOUD_ACCESS_KEY"

[cloud.terraformer.TerraformGCP]
backend_bucket = "suntomi-publishing-generic-terraform"
backend_bucket_prefix = "vault"
root_domain = "suntomi.dev"
project_id = "suntomi"
region = "asia-northeast1"

[vcs.Github]
email = "suntomi.inc@gmail.com"
account = "suntomi-bot"
key = "$DEPLO_VCS_ACCESS_KEY"

[ci.Circle]
key = "$DEPLO_CI_ACCESS_KEY"
    
[client]
org_name = "suntomi, inc."
app_name = "dungeon of zoars"
app_code = "doz"
app_id = "dev.suntomi.app.doz"
client_project_path = "./client"
artifact_path = "/tmp/doz"
version_config_path = "./meta/client/version"

unity_path = "/Applications/Unity_2018.4.2f1/Unity.app/Contents/MacOS/Unity"
serial_code = "$DEPLO_CLIENT_UNITY_SERIAL_CODE"
account = "dokyogames@gmail.com"
password": "$DEPLO_CLIENT_UNITY_ACCOUNT_PASSWORD"

[[client.platform_build_configs.Android]]
"keystore_password": "$DEPLO_CLIENT_ANDROID_KEYSTORE_PASSWORD",
"keyalias_name": "doz",
"keyalias_password": "$DEPLO_CLIENT_ANDROID_KEYALIAS_PASSWORD",
"keystore_path": "./meta/client/Android/user.keystore",
"use_expansion_file": false      

[[client.platform_build_configs.IOS]]
"team_id": "$DEPLO_CLIENT_IOS_TEAM_ID",
"numeric_team_id": "$DEPLO_CLIENT_IOS_NUMERIC_TEAM_ID",
"signing_password": "$DEPLO_CLIENT_IOS_P12_SIGNING_PASSWORD",
"signing_plist_path": "./meta/client/iOS/suntomi_distribution.plist",
"signing_p12_path": "./meta/client/iOS/suntomi_distribution.p12",
"singing_provision_path": "./meta/client/iOS/suntomi_doz_appstore.mobileprovision" 

[[client.stores.Apple]]
account="suntomi.inc@gmail.com",
password="$DEPLO_CLIENT_STORE_APPLE_PASSWORD"

[[client.stores.Google]]
key = "$DEPLO_CLIENT_STORE_GOOGLE_ACCESS_KEY"

[deploy.pr]
"./master-data/.*" = "deplo service deploy master-data-build"

[deploy.release]
"./client/.*" = "deplo service deploy client"

```



### service.toml example
``` toml
# using bash script to get ull control over deployment.
[script]
code = '''
make -C server build IMAGE=$IMAGE
# some script using specific command like gcloud/aws/aliyun/...
docker tag $IMAGE $DEPLO_CONTAINER_REPOSITORY_URL:$DEPLO_PROJECT_ID-game-server #>/dev/null 2>&1 
# you can wrap with deplo exec to dryrun
deplo exec gcloud compute instance-templates create-with-container ...
'''
code = "./path/to/deploy/script.sh"

[script.env]
IMAGE=doz/server

# or specified by parameter
[container.image]
id = "doz/server"
build = "make -C server build"

[container.deploy]
type = "instance" # or "serverless"
ports = [8080, 11111]

[container.deploy.env]  
GOOGLE_APPLICATION_CREDENTIALS = "/path/to/cred.json"

[container.deploy.command_options]
"max-num-replicas" = 64
"min-num-replicas" = 1
"target-cpu-utilization" = 0.5
flags = ["scale-based-on-cpu"]
```