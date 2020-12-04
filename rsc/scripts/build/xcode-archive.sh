#!/bin/bash

xcodebuild_path=${1}
xcode_project_dir=${2}
export_path=${3}
app_name=${4}
scheme_name=${5}
sdk_version=${6}
timestamp=${7}
configuration=${8}
code_sign_identity=${9}
team_id=${10}

echo "---------------- XcodeArchive Start ----------------"
echo "[XcodeBuild]       : "$xcodebuild_path
echo "[XcodeProjectDir]  : "$xcode_project_dir
echo "[XcodeArchivePath] : "$xcode_archive_path
echo "[SchemeName]       : "$scheme_name
echo "[SDK]              : "$sdk_version
echo "[Timestamp]        : "$timestamp
echo "[Configuration]    : "$configuration
echo "[CodeSignIdentity] : "$code_sign_identity
echo "[TeamID]           : "$team_id

xcode_project_path=$xcode_project_dir/$scheme_name.xcodeproj
xcode_archive_path=$xcode_project_dir/Archive/$scheme_name.xcarchive

log_dir=$export_path/log/XcodeArchive
log_path=$log_dir/$timestamp.log
mkdir -p $log_dir

xcode_workspace_path=$(echo $xcode_project_path | sed -e 's/\.xcodeproj$/\.xcworkspace/')
if [ -e "$xcode_workspace_path" ]; then
  # use workspace if it exists
  build_target="-workspace $xcode_workspace_path"
else
  build_target="-project $xcode_project_path"
fi

$xcodebuild_path clean archive\
  $build_target\
  -scheme $scheme_name\
  -sdk $sdk_version\
  -configuration $configuration\
  -archivePath $xcode_archive_path\
  CODE_SIGN_IDENTITY="$code_sign_identity"\
  DEVELOPMENT_TEAM="$team_id"

xcodebuild_result=$?

if [ $xcodebuild_result -eq 0 ]; then
  echo "XcodeArchive Success"
else
  echo "XcodeArchive Failure"
fi

echo "---------------- XcodeArchive End ($xcodebuild_result)----------------"

exit $xcodebuild_result
