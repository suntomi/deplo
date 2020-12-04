#!/bin/bash

unity_version=${1}
project_path=${2}
export_path=${3}
execute_method=${4}
build_target=${6}
environment=${7}
configuration=${8}
define=${9}
timestamp=${10}

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

# run unity with batch mode
$unity_path\
 -batchmode\
 -nographics\
 -projectPath $project_path\
 -buildTarget $build_target\
 -executeMethod $execute_method\
 -logFile $log_path\
 -exportpath $export_path\
 -configuration $configuration\
 -timestamp $timestamp\
 -environment $environment\
 -define $define\
 -quit

unitybuild_result=$?

if [ $unitybuild_result -eq 0 ]; then
  echo "UnityBuild Success"
else
  echo "UnityBuild Failure"
fi

echo "---------------- UnityBuild End ($unitybuild_result)----------------"