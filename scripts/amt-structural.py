"""Bounded exploratory hidden-state statistics and layer-output ablations."""
import json
import torch

def inspect_activations(model, tokenizer, output):
    prompts=["The capital of France is","The capital of Italy is","The diesel engine uses","The brake pedal controls","Two plus three equals","Seven times eight equals"]
    layers=model.model.layers
    ids=sorted(set([0,len(layers)//2,len(layers)-1]))
    records=[]
    feature_records=[]
    with torch.inference_mode():
        for prompt in prompts:
            inputs=tokenizer(prompt,return_tensors="pt",truncation=True,max_length=64)
            base=model(**inputs,output_hidden_states=True,use_cache=False)
            logits=base.logits[0,-1].float()
            top=logits.topk(5).indices
            stats=[]
            for i in ids:
                a=base.hidden_states[i+1][0,-1].float()
                stats.append({"layer":i,"dimension":a.numel(),"mean":a.mean().item(),"rms":a.square().mean().sqrt().item(),"zero_fraction":a.eq(0).float().mean().item()})
            interventions=[]
            for i in ids:
                def zero_hook(module,inputs,value):
                    if isinstance(value,tuple): return (torch.zeros_like(value[0]),)+value[1:]
                    return torch.zeros_like(value)
                handle=layers[i].register_forward_hook(zero_hook)
                try: changed=model(**inputs,use_cache=False).logits[0,-1].float()
                finally: handle.remove()
                p=logits.log_softmax(-1);q=changed.log_softmax(-1)
                interventions.append({"layer":i,"kind":"zero_entire_layer_output","kl_baseline_to_intervention":(p.exp()*(p-q)).sum().item(),"top1_changed":bool(logits.argmax()!=changed.argmax())})
            records.append({"prompt":prompt,"top_tokens":[tokenizer.decode([int(t)]) for t in top],"activations":stats,"interventions":interventions})
    # Compare selected high-magnitude coordinates with deterministic size-matched controls.
    with torch.inference_mode():
        for prompt in prompts:
            inputs=tokenizer(prompt,return_tensors="pt",truncation=True,max_length=64)
            base=model(**inputs,output_hidden_states=True,use_cache=False)
            logits=base.logits[0,-1].float(); p=logits.log_softmax(-1)
            i=len(layers)//2
            a=base.hidden_states[i+1][0,-1].float()
            count=min(16,a.numel())
            selected=a.abs().topk(count).indices
            excluded=set(selected.tolist())
            control=torch.tensor([j for j in range(a.numel()) if j not in excluded][:count],dtype=torch.long)
            effects=[]
            ranked=a.abs().argsort(descending=True)
            near=torch.tensor([j for j in ranked.tolist() if j not in excluded][:count],dtype=torch.long)
            for name,coords in [("largest_absolute_activation",selected),("fixed_disjoint_control",control),("nearest_disjoint_magnitude_control",near)]:
                def mask_hook(module,inputs,value):
                    hidden=value[0] if isinstance(value,tuple) else value
                    changed=hidden.clone(); changed[:,-1,coords]=0
                    return (changed,)+value[1:] if isinstance(value,tuple) else changed
                handle=layers[i].register_forward_hook(mask_hook)
                try: changed=model(**inputs,use_cache=False).logits[0,-1].float()
                finally: handle.remove()
                q=changed.log_softmax(-1)
                effects.append({"kind":name,"coordinates":coords.tolist(),"removed_l2":a[coords].norm().item(),"relative_removed_l2":a[coords].norm().item()/max(a[selected].norm().item(),1e-12),"kl":(p.exp()*(p-q)).sum().item(),"top1_changed":bool(logits.argmax()!=changed.argmax())})
            feature_records.append({"prompt":prompt,"layer":i,"effects":effects})
    output.write_text(json.dumps({"coordinate_ablation":feature_records,"scope":"exploratory layer-output ablation, not feature decoding; nearest disjoint magnitudes are not necessarily matched","records":records},indent=2))
