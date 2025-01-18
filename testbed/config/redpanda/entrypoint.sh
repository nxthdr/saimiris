#!/bin/bash

# Start Redpanda in the background
set -m
/usr/bin/rpk redpanda start --check=false --overprovisioned --smp 1 --memory 500M &

# Wait for Redpanda to be ready
sleep 3

# Create `admin` superuser
/usr/bin/rpk cluster config set superusers ['admin']
/usr/bin/rpk security user create admin  -p 'admin' --mechanism=SCRAM-SHA-512

# Enable SASL
/usr/bin/rpk cluster config set enable_sasl true

# Create `osiris` user and grant it access to the cluster and the `osiris-results` topic
/usr/bin/rpk security user create osiris -p 'osiris' --mechanism SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --cluster -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --topic osiris-results -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
fg %1