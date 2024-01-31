#!/bin/bash

token_file_path=$1

response=$(aws sts assume-role-with-web-identity --role-arn ${role_arn} \
    --region ${region} \
    --role-session-name "ghaction" \
    --web-identity-token $(cat ${token_file_path}) \
    --duration-seconds 120)

echo "export AWS_ACCESS_KEY_ID=$(echo "${response}" | jq -jr ".Credentials.AccessKeyId")"
echo "export AWS_SECRET_ACCESS_KEY=$(echo "${response}" | jq -jr ".Credentials.SecretAccessKey")"
echo "export AWS_SESSION_TOKEN=$(echo "${response}" | jq -jr ".Credentials.SessionToken")"
