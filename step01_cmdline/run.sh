#!/bin/bash

set -e

docker build -f ../Dockerfile.template . -t demo
docker run --rm -t demo --a 1 --b 2 && echo OK
docker run --rm -t demo --a 3 --b 4 && echo OK
docker run --rm -t demo --a 3 --b 4 --op mul && echo OK
docker run --rm -t demo --a 100 --b 100 && (echo "Error, should overflow."; exit 1)
echo "Oveflow handled correctly."
