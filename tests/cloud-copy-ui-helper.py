#!/usr/bin/python3
import json, os, sys, time
mode = os.environ['FLEA_COPY_CASE']
if sys.argv[1] == '--cloud-targets':
    print(json.dumps({'targets': [{'id': 'test', 'label': 'Test', 'destination': 'fixture:test'}]}), flush=True)
    sys.exit()
def event(state):
    print(json.dumps({'state': state, 'message': state, 'bytes': 1, 'total': 2, 'speed': 1}), flush=True)
event('preparing')
if mode == 'cancelrace':
    sys.stdin.readline()
    event('done')
    sys.exit()
if mode in ['cancel', 'eof']:
    sys.stdin.readline()
    event('cancelled')
    sys.exit(1)
if mode == 'badexit':
    sys.exit()
for state in ['uploading', 'verifying', 'done']:
    time.sleep(.05)
    event(state)
if mode == 'terminalcancel': time.sleep(.3)
