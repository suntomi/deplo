#!/bin/bash

# ------------------------------
# environment variables
# ------------------------------
# unity_version=${unity_version}
# project_path=${project_path}
# export_path=${export_path}
# execute_method=${execute_method}
# build_target=${build_target}
# environment=${environment}
# configuration=${configuration}
# define=${define}
# timestamp=${timestamp}

# unity_serial_code=${unity_serial_code}
# unity_account=${unity_serial_code}
# unity_password=${unity_password}

echo "---------------- UnityBuild Start ----------------"
echo "[UnityVersion]          : "$unity_version
echo "[ProjectPath]           : "$project_path
echo "[ExportPath]            : "$export_path
echo "[ExecuteMethod]         : "$execute_method
echo "[Timestamp]             : "$timestamp
echo "[BuildTarget]           : "$build_target
echo "[Environment]           : "$environment
echo "[Configuration]         : "$configuration
echo "[Define]                : "$define

log_dir=$export_path/log/$build_target
log_path=$log_dir/$timestamp.log
mkdir -p $log_dir

# install unity if not exists
unity_path=/Applications/DeploTools/Unity/$unity_version
if [ ! -e "$unity_path" ]; then

  script_dir=$(cd $(dirname $0); pwd)

  brew tap sttz/homebrew-tap
  brew install install-unity
  unity_short_version=$(echo "$unity_version" | sed -n 's/\([0-9]*\.[0-9]*\).*/\1/p')

  sudo install-unity install $unity_version -y -p Unity -p iOS -p Android --opt progressBar=false

  mkdir -p /Applications/DeploTools/Unity
  mv "/Applications/Unity $unity_short_version" $unity_path
fi

# activate license
unity_license_dir="/Library/Application Support/Unity"
sudo mkdir -p "$unity_license_dir"

echo "---- activate unity license ----"
set +e
sudo $unity_version -quit -batchmode \
    -serial "$unity_serial_code" -username "$unity_account" -password "$unity_password" \
    -logFile - -buildTarget $platform -projectPath $project_path

echo "---- restore file ownership ----"
sudo chown -R $USER:staff /Users/$USER/Library/Unity
sudo chown -R $USER:staff /Users/$USER/workdir/client/Unity

echo "---- restore assetdatabase if necessary ----"
$unity_version -quit -batchmode \
    -logFile - -buildTarget $platform -projectPath $project_path

result=$?
if [ $result -ne 0 ]; then
    echo "restore assetdatabase failure with code:$result. try to return license"
    sudo $unity_version -quit -batchmode -returnlicense -projectPath $project_path
    exit $result
fi

# run unity with batch mode
DEPLO_UNITY_PATH=$unity_path \
DEPLO_UNITY_BUILD_EXPORT_PATH=$export_path \
DEPLO_UNITY_BUILD_PROFILE=$configuration \
DEPLO_UNITY_BUILD_TIMESTAMP=$timestamp \
DEPLO_UNITY_BUILD_ENVIRONMENT=$environment \
DEPLO_UNITY_BUILD_SCRIPTING_SYMBOLS=$define \
$unity_path\
 -batchmode\
 -nographics\
 -projectPath $project_path\
 -buildTarget $build_target\
 -executeMethod $execute_method\
 -logFile $log_path\
 -quit

unitybuild_result=$?

if [ $unitybuild_result -eq 0 ]; then
  echo "UnityBuild Success"
else
  echo "UnityBuild Failure"
fi

echo "---------------- UnityBuild End ($unitybuild_result)----------------"