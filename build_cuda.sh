#!/bin/bash
docker build -f Dockerfile.cuda -t rust-cuda .
docker run -it -v $PWD:/root/rust-cuda rust-cuda /root/rust-cuda/build_docker.sh
# docker run -it -v $PWD:/root/rust-cuda --entrypoint /bin/bash rust-cuda 