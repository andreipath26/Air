#include "llama.h"
#include "ggml.h"
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <string>
#include <vector>

static bool read_exact(char *p,size_t n){std::cin.read(p,(std::streamsize)n);return (size_t)std::cin.gcount()==n;}
static bool read_u32(uint32_t &v){char b[4];if(!read_exact(b,4))return false;v=(uint8_t)b[0]|((uint32_t)(uint8_t)b[1]<<8)|((uint32_t)(uint8_t)b[2]<<16)|((uint32_t)(uint8_t)b[3]<<24);return true;}
static bool read_f32(float &v){uint32_t x;if(!read_u32(x))return false;std::memcpy(&v,&x,4);return true;}
static void write_u32(uint32_t v){char b[4]={(char)v,(char)(v>>8),(char)(v>>16),(char)(v>>24)};std::cout.write(b,4);}
static void write_u64(uint64_t v){for(int i=0;i<8;i++)std::cout.put((char)(v>>(8*i)));}
static void error_reply(uint32_t code,const std::string &message,uint64_t us=0,uint32_t tokens=0){write_u32(code);write_u32((uint32_t)message.size());write_u64(us);write_u32(tokens);std::cout.write(message.data(),message.size());std::cout.flush();}
static bool push_batch(llama_context *ctx,llama_batch_ext *batch,const std::vector<llama_token>&tokens,llama_pos pos,bool logits){
    llama_batch_ext_clear(batch);
    for(size_t i=0;i<tokens.size();++i){int32_t ix=llama_batch_ext_add_token(batch,0,tokens[i]);if(ix<0)return false;
      llama_pos p=pos+(llama_pos)i;if(!llama_batch_ext_set_pos(batch,ix,&p))return false;
      if(i+1==tokens.size()&&logits&&!llama_batch_ext_set_output_logits(batch,ix,true))return false;}
    return llama_process(ctx,LLAMA_PROCESS_TYPE_DECODE,batch)==0;
}
int main(int argc,char **argv){
    if(argc!=4)return 2;
    const uint32_t n_ctx=(uint32_t)std::stoul(argv[2]);const int32_t n_threads=std::stoi(argv[3]);
    if(n_ctx==0||n_ctx>4096||n_threads<1||n_threads>4)return 3;
    llama_backend_init();
    auto mp=llama_model_default_params();mp.n_gpu_layers=0;mp.load_mode=LLAMA_LOAD_MODE_MMAP;
    llama_model *model=llama_model_load_from_file(argv[1],mp);if(!model)return 4;
    auto cp=llama_context_default_params();cp.n_ctx=n_ctx;cp.n_batch=std::min<uint32_t>(n_ctx,512);cp.n_ubatch=cp.n_batch;
    cp.n_threads=n_threads;cp.n_threads_batch=n_threads;cp.type_k=GGML_TYPE_Q8_0;cp.type_v=GGML_TYPE_Q8_0;cp.offload_kqv=false;
    llama_context *ctx=llama_init_from_model(model,cp);if(!ctx){llama_model_free(model);return 5;}
    const llama_vocab *vocab=llama_model_get_vocab(model);std::vector<llama_token> cached;
    std::cout.write("AIR1",4);std::cout.flush();
    for(;;){
      uint32_t sl,ul,gl,max_tokens;float temp,top_p;
      if(!read_u32(sl))break;
      if(!read_u32(ul)||!read_u32(gl)||!read_u32(max_tokens)||!read_f32(temp)||!read_f32(top_p)){error_reply(1,"truncated request");break;}
      if(sl>8192||ul>16384||gl>8192||max_tokens==0||max_tokens>n_ctx){error_reply(2,"request exceeds worker bounds");continue;}
      std::string system(sl,'\0'),user(ul,'\0'),grammar(gl,'\0');
      if(!read_exact(system.data(),sl)||!read_exact(user.data(),ul)||!read_exact(grammar.data(),gl)){error_reply(1,"truncated prompt");break;}
      const llama_chat_message messages[2]={{"system",system.c_str()},{"user",user.c_str()}};
      const char *tmpl=llama_model_chat_template(model,nullptr);
      std::vector<char> formatted(32768);
      int32_t plen=llama_chat_apply_template(tmpl,messages,2,true,formatted.data(),(int32_t)formatted.size());
      if(plen<0||(size_t)plen>=formatted.size()){error_reply(3,"chat template application failed");continue;}
      std::string prompt(formatted.data(),(size_t)plen);
      int32_t need=llama_tokenize(vocab,prompt.c_str(),(int32_t)prompt.size(),nullptr,0,true,true);
      if(need>=0){error_reply(4,"tokenizer sizing failed");continue;}
      std::vector<llama_token> tokens((size_t)-need);
      int32_t got=llama_tokenize(vocab,prompt.c_str(),(int32_t)prompt.size(),tokens.data(),(int32_t)tokens.size(),true,true);
      if(got<0){error_reply(4,"prompt tokenization failed");continue;}tokens.resize((size_t)got);
      if(tokens.size()+max_tokens>n_ctx){error_reply(5,"prompt plus generation exceeds context cap");continue;}
      size_t common=0;while(common<cached.size()&&common<tokens.size()&&cached[common]==tokens[common])common++;
      if(!llama_memory_seq_rm(llama_get_memory(ctx),0,(llama_pos)common,-1)){llama_memory_clear(llama_get_memory(ctx),true);cached.clear();common=0;}
      std::vector<llama_token> context(cached.begin(),cached.begin()+common);
      llama_batch_ext *batch=llama_batch_ext_init(ctx);if(!batch){error_reply(6,"batch allocation failed");continue;}
      int32_t batch_cap=(int32_t)std::max<uint32_t>(1,llama_n_batch(ctx));
      llama_pos pos=(llama_pos)common;bool failed=false;
      for(size_t at=common;at<tokens.size();){
        size_t end=std::min(tokens.size(),at+(size_t)batch_cap);
        std::vector<llama_token> part(tokens.begin()+at,tokens.begin()+end);
        if(!push_batch(ctx,batch,part,pos,end==tokens.size())){failed=true;break;}
        context.insert(context.end(),part.begin(),part.end());pos+=(llama_pos)part.size();at=end;
      }
      if(failed){llama_batch_ext_free(batch);error_reply(7,"prompt decode failed");continue;}
      auto sp=llama_sampler_chain_default_params();llama_sampler *sampler=llama_sampler_chain_init(sp);
      if(!grammar.empty())llama_sampler_chain_add(sampler,llama_sampler_init_grammar(vocab,grammar.c_str(),"root"));
      llama_sampler_chain_add(sampler,llama_sampler_init_top_p(std::clamp(top_p,0.01f,1.0f),1));
      llama_sampler_chain_add(sampler,llama_sampler_init_temp(std::clamp(temp,0.0f,2.0f)));
      llama_sampler_chain_add(sampler,llama_sampler_init_dist(0xA17));
      std::string result;uint32_t made=0;int64_t started=ggml_time_us();
      bool output_failed=false;
      for(uint32_t i=0;i<max_tokens;i++){
        llama_token tok=llama_sampler_sample(sampler,ctx,-1);if(llama_vocab_is_eog(vocab,tok))break;
        char piece[8192];int32_t n=llama_token_to_piece(vocab,tok,piece,sizeof(piece),0,true);
        if(n<0||result.size()+(size_t)n>32768){output_failed=true;break;}
        result.append(piece,(size_t)n);made++;
        if(i+1<max_tokens){std::vector<llama_token> one{tok};if(!push_batch(ctx,batch,one,pos,true)){output_failed=true;break;}context.push_back(tok);pos++;}
      }
      uint64_t elapsed=(uint64_t)(ggml_time_us()-started);llama_sampler_free(sampler);llama_batch_ext_free(batch);
      if(output_failed){error_reply(8,"generation failed or exceeded output bounds",elapsed,made);continue;}
      cached=std::move(context);
      write_u32(0);write_u32((uint32_t)result.size());write_u64(elapsed);write_u32(made);std::cout.write(result.data(),result.size());std::cout.flush();
    }
    llama_free(ctx);llama_model_free(model);llama_backend_free();return 0;
}
