import angr

project = angr.Project("betaflight.elf", auto_load_libs=False)
cfg = project.analyses.CFGFast()

bb_addrs = sorted({block.addr for block in cfg.graph.nodes()})

with open("valid_basic_blocks.txt", "w") as f:
    for addr in bb_addrs:
        f.write(f"{addr-1:x}\n")

print(f"Extracted {len(bb_addrs)} basic blocks")
