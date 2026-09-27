    .text
    .globl ix_luaA_pushobject
    .p2align 4
ix_luaA_pushobject:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $0, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_barrierback
    .p2align 4
ix_luaC_barrierback:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $1, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_barrierf
    .p2align 4
ix_luaC_barrierf:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $2, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_callGCTM
    .p2align 4
ix_luaC_callGCTM:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $3, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_freeall
    .p2align 4
ix_luaC_freeall:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $4, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_fullgc
    .p2align 4
ix_luaC_fullgc:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $5, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_link
    .p2align 4
ix_luaC_link:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $6, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_linkupval
    .p2align 4
ix_luaC_linkupval:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $7, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_separateudata
    .p2align 4
ix_luaC_separateudata:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $8, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaC_step
    .p2align 4
ix_luaC_step:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $9, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_call
    .p2align 4
ix_luaD_call:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $10, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_callhook
    .p2align 4
ix_luaD_callhook:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $11, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_growstack
    .p2align 4
ix_luaD_growstack:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $12, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_poscall
    .p2align 4
ix_luaD_poscall:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $13, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_precall
    .p2align 4
ix_luaD_precall:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $14, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_protectedparser
    .p2align 4
ix_luaD_protectedparser:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $15, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_rawrunprotected
    .p2align 4
ix_luaD_rawrunprotected:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $16, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_reallocCI
    .p2align 4
ix_luaD_reallocCI:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $17, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_reallocstack
    .p2align 4
ix_luaD_reallocstack:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $18, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_seterrorobj
    .p2align 4
ix_luaD_seterrorobj:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $19, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaD_throw
    .p2align 4
ix_luaD_throw:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $20, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaE_freethread
    .p2align 4
ix_luaE_freethread:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $21, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaE_newthread
    .p2align 4
ix_luaE_newthread:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $22, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_close
    .p2align 4
ix_luaF_close:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $23, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_findupval
    .p2align 4
ix_luaF_findupval:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $24, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_freeclosure
    .p2align 4
ix_luaF_freeclosure:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $25, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_freeproto
    .p2align 4
ix_luaF_freeproto:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $26, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_freeupval
    .p2align 4
ix_luaF_freeupval:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $27, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_newCclosure
    .p2align 4
ix_luaF_newCclosure:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $28, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_newLclosure
    .p2align 4
ix_luaF_newLclosure:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $29, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_newproto
    .p2align 4
ix_luaF_newproto:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $30, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaF_newupval
    .p2align 4
ix_luaF_newupval:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $31, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaG_checkcode
    .p2align 4
ix_luaG_checkcode:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $32, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaG_checkopenop
    .p2align 4
ix_luaG_checkopenop:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $33, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaG_concaterror
    .p2align 4
ix_luaG_concaterror:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $34, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaG_errormsg
    .p2align 4
ix_luaG_errormsg:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $35, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaG_runerror
    .p2align 4
ix_luaG_runerror:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $36, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_free
    .p2align 4
ix_luaH_free:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $37, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_get
    .p2align 4
ix_luaH_get:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $38, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_getn
    .p2align 4
ix_luaH_getn:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $39, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_getnum
    .p2align 4
ix_luaH_getnum:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $40, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_getstr
    .p2align 4
ix_luaH_getstr:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $41, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_isdummy
    .p2align 4
ix_luaH_isdummy:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $42, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_mainposition
    .p2align 4
ix_luaH_mainposition:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $43, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_new
    .p2align 4
ix_luaH_new:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $44, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_next
    .p2align 4
ix_luaH_next:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $45, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_resizearray
    .p2align 4
ix_luaH_resizearray:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $46, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_set
    .p2align 4
ix_luaH_set:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $47, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_setnum
    .p2align 4
ix_luaH_setnum:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $48, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaH_setstr
    .p2align 4
ix_luaH_setstr:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $49, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_checkstack
    .p2align 4
ix_luaK_checkstack:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $50, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_codeABC
    .p2align 4
ix_luaK_codeABC:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $51, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_codeABx
    .p2align 4
ix_luaK_codeABx:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $52, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_concat
    .p2align 4
ix_luaK_concat:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $53, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_dischargevars
    .p2align 4
ix_luaK_dischargevars:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $54, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_exp2RK
    .p2align 4
ix_luaK_exp2RK:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $55, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_exp2anyreg
    .p2align 4
ix_luaK_exp2anyreg:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $56, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_exp2nextreg
    .p2align 4
ix_luaK_exp2nextreg:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $57, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_exp2val
    .p2align 4
ix_luaK_exp2val:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $58, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_fixline
    .p2align 4
ix_luaK_fixline:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $59, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_getlabel
    .p2align 4
ix_luaK_getlabel:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $60, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_goiftrue
    .p2align 4
ix_luaK_goiftrue:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $61, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_indexed
    .p2align 4
ix_luaK_indexed:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $62, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_infix
    .p2align 4
ix_luaK_infix:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $63, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_jump
    .p2align 4
ix_luaK_jump:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $64, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_nil
    .p2align 4
ix_luaK_nil:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $65, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_numberK
    .p2align 4
ix_luaK_numberK:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $66, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_patchlist
    .p2align 4
ix_luaK_patchlist:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $67, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_patchtohere
    .p2align 4
ix_luaK_patchtohere:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $68, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_posfix
    .p2align 4
ix_luaK_posfix:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $69, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_prefix
    .p2align 4
ix_luaK_prefix:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $70, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_reserveregs
    .p2align 4
ix_luaK_reserveregs:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $71, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_ret
    .p2align 4
ix_luaK_ret:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $72, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_self
    .p2align 4
ix_luaK_self:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $73, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_setlist
    .p2align 4
ix_luaK_setlist:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $74, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_setoneret
    .p2align 4
ix_luaK_setoneret:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $75, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_setreturns
    .p2align 4
ix_luaK_setreturns:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $76, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_storevar
    .p2align 4
ix_luaK_storevar:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $77, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaK_stringK
    .p2align 4
ix_luaK_stringK:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $78, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaM_toobig
    .p2align 4
ix_luaM_toobig:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $79, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_chunkid
    .p2align 4
ix_luaO_chunkid:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $80, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_fb2int
    .p2align 4
ix_luaO_fb2int:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $81, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_int2fb
    .p2align 4
ix_luaO_int2fb:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $82, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_log2
    .p2align 4
ix_luaO_log2:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $83, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_pushfstring
    .p2align 4
ix_luaO_pushfstring:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $84, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_rawequalObj
    .p2align 4
ix_luaO_rawequalObj:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $85, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaO_str2d
    .p2align 4
ix_luaO_str2d:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $86, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaS_newlstr
    .p2align 4
ix_luaS_newlstr:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $87, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaS_newudata
    .p2align 4
ix_luaS_newudata:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $88, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaS_resize
    .p2align 4
ix_luaS_resize:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $89, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaT_gettm
    .p2align 4
ix_luaT_gettm:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $90, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaT_init
    .p2align 4
ix_luaT_init:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $91, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaU_dump
    .p2align 4
ix_luaU_dump:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $92, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaU_header
    .p2align 4
ix_luaU_header:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $93, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaU_print
    .p2align 4
ix_luaU_print:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $94, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaU_undump
    .p2align 4
ix_luaU_undump:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $95, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_concat
    .p2align 4
ix_luaV_concat:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $96, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_equalval
    .p2align 4
ix_luaV_equalval:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $97, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_execute
    .p2align 4
ix_luaV_execute:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $98, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_lessthan
    .p2align 4
ix_luaV_lessthan:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $99, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_tonumber
    .p2align 4
ix_luaV_tonumber:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $100, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaV_tostring
    .p2align 4
ix_luaV_tostring:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $101, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_init
    .p2align 4
ix_luaX_init:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $102, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_lexerror
    .p2align 4
ix_luaX_lexerror:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $103, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_lookahead
    .p2align 4
ix_luaX_lookahead:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $104, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_newstring
    .p2align 4
ix_luaX_newstring:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $105, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_next
    .p2align 4
ix_luaX_next:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $106, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_syntaxerror
    .p2align 4
ix_luaX_syntaxerror:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $107, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaX_token2str
    .p2align 4
ix_luaX_token2str:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $108, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaZ_fill
    .p2align 4
ix_luaZ_fill:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $109, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaZ_lookahead
    .p2align 4
ix_luaZ_lookahead:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $110, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaZ_openspace
    .p2align 4
ix_luaZ_openspace:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $111, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
    .globl ix_luaZ_read
    .p2align 4
ix_luaZ_read:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov $112, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
