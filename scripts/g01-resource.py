"""Linux whole-machine used-memory and optional CPU package energy diagnostics.
Package RAPL is not whole-system electrical energy.
"""
from pathlib import Path
def memory_used():
    m={}
    for line in Path("/proc/meminfo").read_text().splitlines():
        k,_,v=line.partition(":")
        if k in ("MemTotal","MemAvailable"):m[k]=int(v.split()[0])*1024
    return m["MemTotal"]-m["MemAvailable"]
def rapl_domains():
    domains=[]
    for p in sorted(Path("/sys/class/powercap").glob("intel-rapl:*")):
        if p.name.count(":")!=1:continue
        try:
            name=(p/"name").read_text().strip()
            if not name.startswith("package"):continue
            domains.append((p,int((p/"max_energy_range_uj").read_text()),int((p/"energy_uj").read_text())))
        except (OSError,ValueError):continue
    return domains
class Meter:
    def __init__(self):
        self.memory_baseline=memory_used();self.memory_peak=self.memory_baseline
        self.domains=rapl_domains();self.energy_uj=0;self.energy_valid=bool(self.domains)
    def sample(self):
        self.memory_peak=max(self.memory_peak,memory_used())
        next_domains=[]
        for p,maximum,previous in self.domains:
            try:
                current=int((p/"energy_uj").read_text())
                delta=current-previous if current>=previous else maximum-previous+current
                if not 0<=delta<=maximum:raise ValueError("counter range")
                self.energy_uj+=delta;next_domains.append((p,maximum,current))
            except (OSError,ValueError):self.energy_valid=False
        self.domains=next_domains
    def report(self):
        return {"system_used_memory_baseline_bytes":self.memory_baseline,"system_used_memory_peak_bytes":self.memory_peak,"system_used_memory_peak_delta_bytes":max(0,self.memory_peak-self.memory_baseline),"cpu_package_energy_joules":self.energy_uj/1e6 if self.energy_valid else None,"memory_boundary":"Linux MemTotal minus MemAvailable, sampled; includes other processes and kernel","energy_boundary":"Intel RAPL package domains only; not whole-system electrical energy"}
