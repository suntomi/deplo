#!/bin/sh

xcodebuild_path=${1}
xcode_project_dir=${2}
export_path=${3}
app_name=${4}
scheme_name=${5}
timestamp=${6}
option_plist_path=${7}
configuration=${8}

echo "---------------- XcodeExport Start ----------------"
echo "[XcodeBuild]       : "$xcodebuild_path
echo "[XcodeProjectPath] : "$xcode_project_path
echo "[XcodeArchivePath] : "$xcode_archive_path
echo "[SchemeName]       : "$scheme_name
echo "[ExportDir]        : "$export_dir
echo "[OutputPath]       : "$output_path
echo "[Timestamp]        : "$timestamp
echo "[OptionListPath]   : "$option_plist_path
echo "[Configuration]    : "$configuration

xcode_archive_path=$xcode_project_dir/Archive/$scheme_name.xcarchive
export_dir=$xcode_project_dir/Export
output_path=$export_path/iOS/$app_name.ipa

log_dir=$export_path/log/XcodeExport
log_path=$log_dir/$timestamp.log
mkdir -p $log_dir

$xcodebuild_path -exportArchive\
  -archivePath $xcode_archive_path\
  -exportOptionsPlist $option_plist_path\
  -exportPath $export_dir

xcodeexport_result=$?

cp $export_dir/$scheme_name.ipa $output_path

if [ $xcodeexport_result -eq 0 ]; then
  echo "XcodeExport Success"
else
  echo "XcodeExport Failure"
fi

echo "---------------- XcodeExport End ($xcodeexport_result}----------------"

exit $xcodeexport_result
