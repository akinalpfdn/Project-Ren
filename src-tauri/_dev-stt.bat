@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
set "LIBCLANG_PATH=C:\Program Files\LLVM\bin"
set "CUDA_PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6"
set "CUDA_PATH_V12_6=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6"
set "CudaToolkitDir=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6\"
set "PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6\bin;C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6\libnvvp;%PATH%"
cd /d "C:\Users\akina\source\repos\Project-Ren"
npm run tauri dev -- --features stt
