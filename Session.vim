let SessionLoad = 1
if &cp | set nocp | endif
let s:so_save = &so | let s:siso_save = &siso | set so=0 siso=0
let v:this_session=expand("<sfile>:p")
silent only
cd ~/Desktop/shell_stuff/conch-runtime
if expand('%') == '' && !&modified && line('$') <= 1 && getline(1) == ''
  let s:wipebuf = bufnr('%')
endif
set shortmess=aoO
badd +8 src/env/mod.rs
badd +32 src/spawn/builtin/mod.rs
badd +98 src/spawn/func_exec.rs
badd +65 src/env/func.rs
badd +78 CHANGELOG.md
badd +168 src/spawn/pipeline.rs
badd +7 src/spawn/builtin/pwd.rs
badd +83 examples/shell.rs
badd +702 ~/.rustup/toolchains/nightly-x86_64-apple-darwin/lib/rustlib/src/rust/src/libstd/thread/mod.rs
badd +64 ~/.rustup/toolchains/nightly-x86_64-apple-darwin/lib/rustlib/src/rust/src/libcore/time.rs
badd +1 src/env/async_io/mod.rs
badd +92 src/spawn/builtin/shift.rs
badd +1 tests/echo.rs
badd +1 src/spawn/builtin/echo.rs
badd +48 src/exit_status.rs
badd +233 src/spawn/mod.rs
badd +343 src/env/builtin.rs
badd +178 src/spawn/builtin/cd.rs
badd +157 src/sys/unix/fd_manager.rs
badd +286 tests/cd.rs
argglobal
silent! argdel *
$argadd src/lib.rs
edit src/spawn/builtin/mod.rs
set splitbelow splitright
set nosplitbelow
set nosplitright
wincmd t
set winminheight=0
set winheight=1
set winminwidth=0
set winwidth=1
argglobal
setlocal fdm=manual
setlocal fde=0
setlocal fmr={{{,}}}
setlocal fdi=#
setlocal fdl=0
setlocal fml=1
setlocal fdn=20
setlocal fen
silent! normal! zE
let s:l = 32 - ((31 * winheight(0) + 44) / 89)
if s:l < 1 | let s:l = 1 | endif
exe s:l
normal! zt
32
normal! 0
tabnext 1
if exists('s:wipebuf') && s:wipebuf != bufnr('%')
  silent exe 'bwipe ' . s:wipebuf
endif
unlet! s:wipebuf
set winheight=1 winwidth=20 shortmess=filnxtToOc
set winminheight=1 winminwidth=1
let s:sx = expand("<sfile>:p:r")."x.vim"
if file_readable(s:sx)
  exe "source " . fnameescape(s:sx)
endif
let &so = s:so_save | let &siso = s:siso_save
doautoall SessionLoadPost
unlet SessionLoad
" vim: set ft=vim :
