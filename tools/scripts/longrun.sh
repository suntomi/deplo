#!/bin/sh 

# trap 'echo trapped. && exit 0' 2

for i in `seq 1 ${1}`; do
    echo $i
    sleep 1
done
