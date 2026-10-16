#!/usr/bin/env python3
import json
import pathlib
import socket
import subprocess
import sys
import threading

root = pathlib.Path(__file__).resolve().parents[1]
binary = root / "target/release/sysly"
evidence = root / ".audit"
evidence.mkdir(exist_ok=True)
server = socket.socket()
server.bind(("127.0.0.1", 0))
server.listen(1)
received = 0


def transfer():
    global received
    client = socket.create_connection(server.getsockname())
    connection, _ = server.accept()

    def consume():
        global received
        while data := connection.recv(65536):
            received += len(data)
        connection.close()

    reader = threading.Thread(target=consume)
    reader.start()
    block = b"s" * 65536
    for _ in range(256):
        client.sendall(block)
    client.close()
    reader.join()


process = subprocess.Popen([str(binary), "--sample", "6", "1"], stdout=subprocess.PIPE, text=True)
samples = []
workload = None
try:
    for line in process.stdout:
        samples.append(json.loads(line))
        if len(samples) == 1:
            transfer()
            workload = subprocess.Popen([sys.executable, "-c", "import time; end=time.monotonic()+3\nwhile time.monotonic()<end: pass"])
    assert process.wait(timeout=15) == 0, "sampler exited with an error"
    assert len(samples) == 6, "missing snapshots"
    expected_memory = int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True))
    expected_cores = int(subprocess.check_output(["sysctl", "-n", "hw.logicalcpu"], text=True))
    assert all(s["memory"]["total"] == expected_memory for s in samples), "memory differs from sysctl"
    assert all(len(s["cores"]) == expected_cores for s in samples), "core count differs from sysctl"
    assert all(0 <= s["cpu_percent"] <= 100 for s in samples), "invalid CPU percentage"
    assert all(s["elapsed_secs"] > a["elapsed_secs"] for a, s in zip(samples, samples[1:])), "timestamps do not advance"
    assert all(s["sequence"] == i + 1 for i, s in enumerate(samples)), "sequence missing"
    assert all(any(p["key"]["pid"] == process.pid for p in s["processes"]) for s in samples), "monitor hides itself"
    assert any(p["key"]["pid"] == workload.pid and p["cpu_percent"] > 20 for s in samples[1:] for p in s["processes"]), "CPU workload was not measured"
    first_loop = next(n for n in samples[0]["networks"] if n["name"] == "lo0")
    last_loop = next(n for n in samples[-1]["networks"] if n["name"] == "lo0")
    assert received == 16777216, "transfer incomplete"
    assert last_loop["total_received"] - first_loop["total_received"] >= received, "loopback counters missed real transfer"
    assert any(n["received_per_sec"] > 1_000_000 for s in samples[1:] for n in s["networks"] if n["name"] == "lo0"), "network rate missed real transfer"
    report = {
        "result": "VERIFIED",
        "samples": len(samples),
        "memory_bytes_matches_sysctl": expected_memory,
        "cores_match_sysctl": expected_cores,
        "transfer_bytes_verified": received,
        "process_counts": [len(s["processes"]) for s in samples],
        "gpu_available": samples[-1]["hardware"]["gpu_percent"] is not None,
        "cpu_temperature_available": samples[-1]["hardware"]["cpu_temp"] is not None,
        "fans": samples[-1]["hardware"]["fans"],
        "battery_available": samples[-1]["battery"] is not None,
        "disk_io_available": any(d["read_per_sec"] is not None for d in samples[-1]["disks"]),
        "hardware_status": samples[-1]["hardware"]["status"],
        "warnings": samples[-1]["warnings"],
    }
    (evidence / "metrics-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
finally:
    server.close()
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=10)
    if workload and workload.poll() is None:
        workload.terminate()
        workload.wait(timeout=10)
