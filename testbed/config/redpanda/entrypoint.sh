#!/bin/bash

# Start Redpanda in the background
set -m
/usr/bin/rpk redpanda start --check=false --overprovisioned --smp 1 --memory 500M &

# Wait for Redpanda to be ready
sleep 3

# Create the topics
/usr/bin/rpk topic create osiris-targets
/usr/bin/rpk topic create osiris-results

# Create `admin` superuser
/usr/bin/rpk cluster config set superusers ['admin']
/usr/bin/rpk security user create admin  -p 'admin' --mechanism=SCRAM-SHA-512

# Enable SASL
/usr/bin/rpk cluster config set enable_sasl true

# Create `osiris` user and grant it access to the cluster and the topic
/usr/bin/rpk security user create osiris -p 'osiris' --mechanism SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --cluster -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --topic osiris-targets -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --group osiris-targets-group -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
/usr/bin/rpk security acl create --allow-principal User:osiris --operation all --topic osiris-results -X user=admin -X pass='admin' -X sasl.mechanism=SCRAM-SHA-512
fg %1