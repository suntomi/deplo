#!/bin/sh 

# trap 'echo trapped. && exit 0' 2

for i in `seq 1 ${1}`; do
    echo "$i times"
    sleep 1
done
