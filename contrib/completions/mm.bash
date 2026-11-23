# Generated from the canonical Clap tree by ./scripts/generate-artifacts.sh. Do not edit.
_mm() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="mm"
                ;;
            mm,access)
                cmd="mm__subcmd__access"
                ;;
            mm,agent)
                cmd="mm__subcmd__agent"
                ;;
            mm,build)
                cmd="mm__subcmd__build"
                ;;
            mm,config)
                cmd="mm__subcmd__config"
                ;;
            mm,deploy)
                cmd="mm__subcmd__deploy"
                ;;
            mm,dev)
                cmd="mm__subcmd__dev"
                ;;
            mm,doctor)
                cmd="mm__subcmd__doctor"
                ;;
            mm,env)
                cmd="mm__subcmd__env"
                ;;
            mm,forward)
                cmd="mm__subcmd__forward"
                ;;
            mm,help)
                cmd="mm__subcmd__help"
                ;;
            mm,history)
                cmd="mm__subcmd__history"
                ;;
            mm,ingress)
                cmd="mm__subcmd__ingress"
                ;;
            mm,init)
                cmd="mm__subcmd__init"
                ;;
            mm,job)
                cmd="mm__subcmd__job"
                ;;
            mm,key)
                cmd="mm__subcmd__key"
                ;;
            mm,mcp)
                cmd="mm__subcmd__mcp"
                ;;
            mm,secret)
                cmd="mm__subcmd__secret"
                ;;
            mm,service)
                cmd="mm__subcmd__service"
                ;;
            mm,target)
                cmd="mm__subcmd__target"
                ;;
            mm,team)
                cmd="mm__subcmd__team"
                ;;
            mm,update)
                cmd="mm__subcmd__update"
                ;;
            mm,workspace)
                cmd="mm__subcmd__workspace"
                ;;
            mm__subcmd__access,add)
                cmd="mm__subcmd__access__subcmd__add"
                ;;
            mm__subcmd__access,attach)
                cmd="mm__subcmd__access__subcmd__attach"
                ;;
            mm__subcmd__access,detach)
                cmd="mm__subcmd__access__subcmd__detach"
                ;;
            mm__subcmd__access,help)
                cmd="mm__subcmd__access__subcmd__help"
                ;;
            mm__subcmd__access,remove)
                cmd="mm__subcmd__access__subcmd__remove"
                ;;
            mm__subcmd__access,session)
                cmd="mm__subcmd__access__subcmd__session"
                ;;
            mm__subcmd__access,show)
                cmd="mm__subcmd__access__subcmd__show"
                ;;
            mm__subcmd__access,user)
                cmd="mm__subcmd__access__subcmd__user"
                ;;
            mm__subcmd__access__subcmd__help,add)
                cmd="mm__subcmd__access__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__access__subcmd__help,attach)
                cmd="mm__subcmd__access__subcmd__help__subcmd__attach"
                ;;
            mm__subcmd__access__subcmd__help,detach)
                cmd="mm__subcmd__access__subcmd__help__subcmd__detach"
                ;;
            mm__subcmd__access__subcmd__help,help)
                cmd="mm__subcmd__access__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__access__subcmd__help,remove)
                cmd="mm__subcmd__access__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__access__subcmd__help,session)
                cmd="mm__subcmd__access__subcmd__help__subcmd__session"
                ;;
            mm__subcmd__access__subcmd__help,show)
                cmd="mm__subcmd__access__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__access__subcmd__help,user)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__session,list)
                cmd="mm__subcmd__access__subcmd__help__subcmd__session__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__session,revoke)
                cmd="mm__subcmd__access__subcmd__help__subcmd__session__subcmd__revoke"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__session,rotate-key)
                cmd="mm__subcmd__access__subcmd__help__subcmd__session__subcmd__rotate__subcmd__key"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,add)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,disable)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__disable"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,enable)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__enable"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,list)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,password-reset)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__password__subcmd__reset"
                ;;
            mm__subcmd__access__subcmd__help__subcmd__user,remove)
                cmd="mm__subcmd__access__subcmd__help__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__access__subcmd__session,help)
                cmd="mm__subcmd__access__subcmd__session__subcmd__help"
                ;;
            mm__subcmd__access__subcmd__session,list)
                cmd="mm__subcmd__access__subcmd__session__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__session,revoke)
                cmd="mm__subcmd__access__subcmd__session__subcmd__revoke"
                ;;
            mm__subcmd__access__subcmd__session,rotate-key)
                cmd="mm__subcmd__access__subcmd__session__subcmd__rotate__subcmd__key"
                ;;
            mm__subcmd__access__subcmd__session__subcmd__help,help)
                cmd="mm__subcmd__access__subcmd__session__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__access__subcmd__session__subcmd__help,list)
                cmd="mm__subcmd__access__subcmd__session__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__session__subcmd__help,revoke)
                cmd="mm__subcmd__access__subcmd__session__subcmd__help__subcmd__revoke"
                ;;
            mm__subcmd__access__subcmd__session__subcmd__help,rotate-key)
                cmd="mm__subcmd__access__subcmd__session__subcmd__help__subcmd__rotate__subcmd__key"
                ;;
            mm__subcmd__access__subcmd__user,add)
                cmd="mm__subcmd__access__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__access__subcmd__user,disable)
                cmd="mm__subcmd__access__subcmd__user__subcmd__disable"
                ;;
            mm__subcmd__access__subcmd__user,enable)
                cmd="mm__subcmd__access__subcmd__user__subcmd__enable"
                ;;
            mm__subcmd__access__subcmd__user,help)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help"
                ;;
            mm__subcmd__access__subcmd__user,list)
                cmd="mm__subcmd__access__subcmd__user__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__user,password-reset)
                cmd="mm__subcmd__access__subcmd__user__subcmd__password__subcmd__reset"
                ;;
            mm__subcmd__access__subcmd__user,remove)
                cmd="mm__subcmd__access__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,add)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,disable)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__disable"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,enable)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__enable"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,help)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,list)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,password-reset)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__password__subcmd__reset"
                ;;
            mm__subcmd__access__subcmd__user__subcmd__help,remove)
                cmd="mm__subcmd__access__subcmd__user__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__agent,help)
                cmd="mm__subcmd__agent__subcmd__help"
                ;;
            mm__subcmd__agent,setup)
                cmd="mm__subcmd__agent__subcmd__setup"
                ;;
            mm__subcmd__agent,status)
                cmd="mm__subcmd__agent__subcmd__status"
                ;;
            mm__subcmd__agent,trust)
                cmd="mm__subcmd__agent__subcmd__trust"
                ;;
            mm__subcmd__agent__subcmd__help,help)
                cmd="mm__subcmd__agent__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__agent__subcmd__help,setup)
                cmd="mm__subcmd__agent__subcmd__help__subcmd__setup"
                ;;
            mm__subcmd__agent__subcmd__help,status)
                cmd="mm__subcmd__agent__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__agent__subcmd__help,trust)
                cmd="mm__subcmd__agent__subcmd__help__subcmd__trust"
                ;;
            mm__subcmd__build,apply)
                cmd="mm__subcmd__build__subcmd__apply"
                ;;
            mm__subcmd__build,help)
                cmd="mm__subcmd__build__subcmd__help"
                ;;
            mm__subcmd__build,import)
                cmd="mm__subcmd__build__subcmd__import"
                ;;
            mm__subcmd__build,plan)
                cmd="mm__subcmd__build__subcmd__plan"
                ;;
            mm__subcmd__build,receipt)
                cmd="mm__subcmd__build__subcmd__receipt"
                ;;
            mm__subcmd__build__subcmd__help,apply)
                cmd="mm__subcmd__build__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__build__subcmd__help,help)
                cmd="mm__subcmd__build__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__build__subcmd__help,import)
                cmd="mm__subcmd__build__subcmd__help__subcmd__import"
                ;;
            mm__subcmd__build__subcmd__help,plan)
                cmd="mm__subcmd__build__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__build__subcmd__help,receipt)
                cmd="mm__subcmd__build__subcmd__help__subcmd__receipt"
                ;;
            mm__subcmd__config,access)
                cmd="mm__subcmd__config__subcmd__access"
                ;;
            mm__subcmd__config,binding)
                cmd="mm__subcmd__config__subcmd__binding"
                ;;
            mm__subcmd__config,build)
                cmd="mm__subcmd__config__subcmd__build"
                ;;
            mm__subcmd__config,environment)
                cmd="mm__subcmd__config__subcmd__environment"
                ;;
            mm__subcmd__config,help)
                cmd="mm__subcmd__config__subcmd__help"
                ;;
            mm__subcmd__config,ingress)
                cmd="mm__subcmd__config__subcmd__ingress"
                ;;
            mm__subcmd__config,job)
                cmd="mm__subcmd__config__subcmd__job"
                ;;
            mm__subcmd__config,join)
                cmd="mm__subcmd__config__subcmd__join"
                ;;
            mm__subcmd__config,module)
                cmd="mm__subcmd__config__subcmd__module"
                ;;
            mm__subcmd__config,mount)
                cmd="mm__subcmd__config__subcmd__mount"
                ;;
            mm__subcmd__config,schedule)
                cmd="mm__subcmd__config__subcmd__schedule"
                ;;
            mm__subcmd__config,schema)
                cmd="mm__subcmd__config__subcmd__schema"
                ;;
            mm__subcmd__config,secret)
                cmd="mm__subcmd__config__subcmd__secret"
                ;;
            mm__subcmd__config,service)
                cmd="mm__subcmd__config__subcmd__service"
                ;;
            mm__subcmd__config,sources)
                cmd="mm__subcmd__config__subcmd__sources"
                ;;
            mm__subcmd__config,split)
                cmd="mm__subcmd__config__subcmd__split"
                ;;
            mm__subcmd__config,stack)
                cmd="mm__subcmd__config__subcmd__stack"
                ;;
            mm__subcmd__config,user)
                cmd="mm__subcmd__config__subcmd__user"
                ;;
            mm__subcmd__config,validate)
                cmd="mm__subcmd__config__subcmd__validate"
                ;;
            mm__subcmd__config__subcmd__access,add)
                cmd="mm__subcmd__config__subcmd__access__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__access,edit)
                cmd="mm__subcmd__config__subcmd__access__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__access,help)
                cmd="mm__subcmd__config__subcmd__access__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__access,remove)
                cmd="mm__subcmd__config__subcmd__access__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__access__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__access__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__access__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__access__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__access__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__access__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__access__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__access__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__binding,add)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__binding,edit)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__binding,help)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__binding,remove)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__binding__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__binding__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__binding__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__binding__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__build,add)
                cmd="mm__subcmd__config__subcmd__build__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__build,edit)
                cmd="mm__subcmd__config__subcmd__build__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__build,help)
                cmd="mm__subcmd__config__subcmd__build__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__build,remove)
                cmd="mm__subcmd__config__subcmd__build__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__build__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__build__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__build__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__build__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__build__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__build__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__build__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__build__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__environment,add)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__environment,edit)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__environment,help)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__environment,remove)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__environment__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__environment__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__environment__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__environment__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help,access)
                cmd="mm__subcmd__config__subcmd__help__subcmd__access"
                ;;
            mm__subcmd__config__subcmd__help,binding)
                cmd="mm__subcmd__config__subcmd__help__subcmd__binding"
                ;;
            mm__subcmd__config__subcmd__help,build)
                cmd="mm__subcmd__config__subcmd__help__subcmd__build"
                ;;
            mm__subcmd__config__subcmd__help,environment)
                cmd="mm__subcmd__config__subcmd__help__subcmd__environment"
                ;;
            mm__subcmd__config__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__help,ingress)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress"
                ;;
            mm__subcmd__config__subcmd__help,job)
                cmd="mm__subcmd__config__subcmd__help__subcmd__job"
                ;;
            mm__subcmd__config__subcmd__help,join)
                cmd="mm__subcmd__config__subcmd__help__subcmd__join"
                ;;
            mm__subcmd__config__subcmd__help,module)
                cmd="mm__subcmd__config__subcmd__help__subcmd__module"
                ;;
            mm__subcmd__config__subcmd__help,mount)
                cmd="mm__subcmd__config__subcmd__help__subcmd__mount"
                ;;
            mm__subcmd__config__subcmd__help,schedule)
                cmd="mm__subcmd__config__subcmd__help__subcmd__schedule"
                ;;
            mm__subcmd__config__subcmd__help,schema)
                cmd="mm__subcmd__config__subcmd__help__subcmd__schema"
                ;;
            mm__subcmd__config__subcmd__help,secret)
                cmd="mm__subcmd__config__subcmd__help__subcmd__secret"
                ;;
            mm__subcmd__config__subcmd__help,service)
                cmd="mm__subcmd__config__subcmd__help__subcmd__service"
                ;;
            mm__subcmd__config__subcmd__help,sources)
                cmd="mm__subcmd__config__subcmd__help__subcmd__sources"
                ;;
            mm__subcmd__config__subcmd__help,split)
                cmd="mm__subcmd__config__subcmd__help__subcmd__split"
                ;;
            mm__subcmd__config__subcmd__help,stack)
                cmd="mm__subcmd__config__subcmd__help__subcmd__stack"
                ;;
            mm__subcmd__config__subcmd__help,user)
                cmd="mm__subcmd__config__subcmd__help__subcmd__user"
                ;;
            mm__subcmd__config__subcmd__help,validate)
                cmd="mm__subcmd__config__subcmd__help__subcmd__validate"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__access,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__access__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__access,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__access__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__access,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__access__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__binding,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__binding,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__binding,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__build,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__build__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__build,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__build__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__build,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__build__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__environment,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__environment,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__environment,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__ingress,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__ingress,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__ingress,explain)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__explain"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__ingress,list)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__list"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__ingress,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__job,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__job__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__job,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__job__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__job,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__job__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__module,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__module__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__module,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__module__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__module,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__module__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__mount,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__mount,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__mount,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__schedule,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__schedule,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__schedule,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__secret,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__secret,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__secret,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__service,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__service__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__service,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__service__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__service,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__service__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__stack,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__stack,compile)
                cmd="mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__compile"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__stack,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__stack,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__user,add)
                cmd="mm__subcmd__config__subcmd__help__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__user,edit)
                cmd="mm__subcmd__config__subcmd__help__subcmd__user__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__help__subcmd__user,remove)
                cmd="mm__subcmd__config__subcmd__help__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__ingress,add)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__ingress,edit)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__ingress,explain)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__explain"
                ;;
            mm__subcmd__config__subcmd__ingress,help)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__ingress,list)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__list"
                ;;
            mm__subcmd__config__subcmd__ingress,remove)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,explain)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__explain"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,list)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__config__subcmd__ingress__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__job,add)
                cmd="mm__subcmd__config__subcmd__job__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__job,edit)
                cmd="mm__subcmd__config__subcmd__job__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__job,help)
                cmd="mm__subcmd__config__subcmd__job__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__job,remove)
                cmd="mm__subcmd__config__subcmd__job__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__job__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__job__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__job__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__job__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__job__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__job__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__job__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__job__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__module,add)
                cmd="mm__subcmd__config__subcmd__module__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__module,edit)
                cmd="mm__subcmd__config__subcmd__module__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__module,help)
                cmd="mm__subcmd__config__subcmd__module__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__module,remove)
                cmd="mm__subcmd__config__subcmd__module__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__module__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__module__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__module__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__module__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__module__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__module__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__module__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__module__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__mount,add)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__mount,edit)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__mount,help)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__mount,remove)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__mount__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__mount__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__mount__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__mount__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__schedule,add)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__schedule,edit)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__schedule,help)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__schedule,remove)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__schedule__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__schedule__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__schedule__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__schedule__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__secret,add)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__secret,edit)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__secret,help)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__secret,remove)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__secret__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__secret__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__secret__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__secret__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__service,add)
                cmd="mm__subcmd__config__subcmd__service__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__service,edit)
                cmd="mm__subcmd__config__subcmd__service__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__service,help)
                cmd="mm__subcmd__config__subcmd__service__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__service,remove)
                cmd="mm__subcmd__config__subcmd__service__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__service__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__service__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__service__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__service__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__service__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__service__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__service__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__service__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__stack,add)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__stack,compile)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__compile"
                ;;
            mm__subcmd__config__subcmd__stack,edit)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__stack,help)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__stack,remove)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__stack__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__stack__subcmd__help,compile)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__compile"
                ;;
            mm__subcmd__config__subcmd__stack__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__stack__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__stack__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__user,add)
                cmd="mm__subcmd__config__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__user,edit)
                cmd="mm__subcmd__config__subcmd__user__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__user,help)
                cmd="mm__subcmd__config__subcmd__user__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__user,remove)
                cmd="mm__subcmd__config__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__config__subcmd__user__subcmd__help,add)
                cmd="mm__subcmd__config__subcmd__user__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__config__subcmd__user__subcmd__help,edit)
                cmd="mm__subcmd__config__subcmd__user__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__config__subcmd__user__subcmd__help,help)
                cmd="mm__subcmd__config__subcmd__user__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__config__subcmd__user__subcmd__help,remove)
                cmd="mm__subcmd__config__subcmd__user__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__deploy,apply)
                cmd="mm__subcmd__deploy__subcmd__apply"
                ;;
            mm__subcmd__deploy,cancel)
                cmd="mm__subcmd__deploy__subcmd__cancel"
                ;;
            mm__subcmd__deploy,help)
                cmd="mm__subcmd__deploy__subcmd__help"
                ;;
            mm__subcmd__deploy,logs)
                cmd="mm__subcmd__deploy__subcmd__logs"
                ;;
            mm__subcmd__deploy,plan)
                cmd="mm__subcmd__deploy__subcmd__plan"
                ;;
            mm__subcmd__deploy,redeploy)
                cmd="mm__subcmd__deploy__subcmd__redeploy"
                ;;
            mm__subcmd__deploy,rollback)
                cmd="mm__subcmd__deploy__subcmd__rollback"
                ;;
            mm__subcmd__deploy,status)
                cmd="mm__subcmd__deploy__subcmd__status"
                ;;
            mm__subcmd__deploy__subcmd__help,apply)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__deploy__subcmd__help,cancel)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__cancel"
                ;;
            mm__subcmd__deploy__subcmd__help,help)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__deploy__subcmd__help,logs)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__logs"
                ;;
            mm__subcmd__deploy__subcmd__help,plan)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__deploy__subcmd__help,redeploy)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__redeploy"
                ;;
            mm__subcmd__deploy__subcmd__help,rollback)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__rollback"
                ;;
            mm__subcmd__deploy__subcmd__help,status)
                cmd="mm__subcmd__deploy__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__dev,dashboard)
                cmd="mm__subcmd__dev__subcmd__dashboard"
                ;;
            mm__subcmd__dev,disable)
                cmd="mm__subcmd__dev__subcmd__disable"
                ;;
            mm__subcmd__dev,down)
                cmd="mm__subcmd__dev__subcmd__down"
                ;;
            mm__subcmd__dev,enable)
                cmd="mm__subcmd__dev__subcmd__enable"
                ;;
            mm__subcmd__dev,help)
                cmd="mm__subcmd__dev__subcmd__help"
                ;;
            mm__subcmd__dev,mounts)
                cmd="mm__subcmd__dev__subcmd__mounts"
                ;;
            mm__subcmd__dev,open)
                cmd="mm__subcmd__dev__subcmd__open"
                ;;
            mm__subcmd__dev,services)
                cmd="mm__subcmd__dev__subcmd__services"
                ;;
            mm__subcmd__dev,setup)
                cmd="mm__subcmd__dev__subcmd__setup"
                ;;
            mm__subcmd__dev,status)
                cmd="mm__subcmd__dev__subcmd__status"
                ;;
            mm__subcmd__dev,tls)
                cmd="mm__subcmd__dev__subcmd__tls"
                ;;
            mm__subcmd__dev,up)
                cmd="mm__subcmd__dev__subcmd__up"
                ;;
            mm__subcmd__dev__subcmd__dashboard,help)
                cmd="mm__subcmd__dev__subcmd__dashboard__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__dashboard,serve)
                cmd="mm__subcmd__dev__subcmd__dashboard__subcmd__serve"
                ;;
            mm__subcmd__dev__subcmd__dashboard__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__dashboard__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__dashboard__subcmd__help,serve)
                cmd="mm__subcmd__dev__subcmd__dashboard__subcmd__help__subcmd__serve"
                ;;
            mm__subcmd__dev__subcmd__help,dashboard)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__dashboard"
                ;;
            mm__subcmd__dev__subcmd__help,disable)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__disable"
                ;;
            mm__subcmd__dev__subcmd__help,down)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__down"
                ;;
            mm__subcmd__dev__subcmd__help,enable)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__enable"
                ;;
            mm__subcmd__dev__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__help,mounts)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__mounts"
                ;;
            mm__subcmd__dev__subcmd__help,open)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__open"
                ;;
            mm__subcmd__dev__subcmd__help,services)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services"
                ;;
            mm__subcmd__dev__subcmd__help,setup)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__setup"
                ;;
            mm__subcmd__dev__subcmd__help,status)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__help,tls)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__tls"
                ;;
            mm__subcmd__dev__subcmd__help,up)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__up"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__dashboard,serve)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__dashboard__subcmd__serve"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__mounts,configure)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__configure"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__mounts,disable)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__disable"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__mounts,plan)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,apply)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__apply"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,plan)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,remove)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__remove"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,repair)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,status)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__services,upgrade)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__upgrade"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__setup,project)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__setup__subcmd__project"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__tls,repair)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__tls,rotate)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__rotate"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__tls,status)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__help__subcmd__tls,uninstall)
                cmd="mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__uninstall"
                ;;
            mm__subcmd__dev__subcmd__mounts,configure)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__configure"
                ;;
            mm__subcmd__dev__subcmd__mounts,disable)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__disable"
                ;;
            mm__subcmd__dev__subcmd__mounts,help)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__mounts,plan)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__mounts__subcmd__help,configure)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__configure"
                ;;
            mm__subcmd__dev__subcmd__mounts__subcmd__help,disable)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__disable"
                ;;
            mm__subcmd__dev__subcmd__mounts__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__mounts__subcmd__help,plan)
                cmd="mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__services,apply)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__apply"
                ;;
            mm__subcmd__dev__subcmd__services,help)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__services,plan)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__services,remove)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__remove"
                ;;
            mm__subcmd__dev__subcmd__services,repair)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__services,status)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__services,upgrade)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__upgrade"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,apply)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,plan)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,remove)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,repair)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,status)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__services__subcmd__help,upgrade)
                cmd="mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__upgrade"
                ;;
            mm__subcmd__dev__subcmd__setup,help)
                cmd="mm__subcmd__dev__subcmd__setup__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__setup,project)
                cmd="mm__subcmd__dev__subcmd__setup__subcmd__project"
                ;;
            mm__subcmd__dev__subcmd__setup__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__setup__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__setup__subcmd__help,project)
                cmd="mm__subcmd__dev__subcmd__setup__subcmd__help__subcmd__project"
                ;;
            mm__subcmd__dev__subcmd__tls,help)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__tls,repair)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__tls,rotate)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__rotate"
                ;;
            mm__subcmd__dev__subcmd__tls,status)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__tls,uninstall)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__uninstall"
                ;;
            mm__subcmd__dev__subcmd__tls__subcmd__help,help)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__dev__subcmd__tls__subcmd__help,repair)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__repair"
                ;;
            mm__subcmd__dev__subcmd__tls__subcmd__help,rotate)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__rotate"
                ;;
            mm__subcmd__dev__subcmd__tls__subcmd__help,status)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__dev__subcmd__tls__subcmd__help,uninstall)
                cmd="mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__uninstall"
                ;;
            mm__subcmd__doctor,help)
                cmd="mm__subcmd__doctor__subcmd__help"
                ;;
            mm__subcmd__doctor,recover-state)
                cmd="mm__subcmd__doctor__subcmd__recover__subcmd__state"
                ;;
            mm__subcmd__doctor__subcmd__help,help)
                cmd="mm__subcmd__doctor__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__doctor__subcmd__help,recover-state)
                cmd="mm__subcmd__doctor__subcmd__help__subcmd__recover__subcmd__state"
                ;;
            mm__subcmd__env,check)
                cmd="mm__subcmd__env__subcmd__check"
                ;;
            mm__subcmd__env,default)
                cmd="mm__subcmd__env__subcmd__default"
                ;;
            mm__subcmd__env,delivery)
                cmd="mm__subcmd__env__subcmd__delivery"
                ;;
            mm__subcmd__env,diff)
                cmd="mm__subcmd__env__subcmd__diff"
                ;;
            mm__subcmd__env,discard)
                cmd="mm__subcmd__env__subcmd__discard"
                ;;
            mm__subcmd__env,explain)
                cmd="mm__subcmd__env__subcmd__explain"
                ;;
            mm__subcmd__env,help)
                cmd="mm__subcmd__env__subcmd__help"
                ;;
            mm__subcmd__env,materialize)
                cmd="mm__subcmd__env__subcmd__materialize"
                ;;
            mm__subcmd__env,review)
                cmd="mm__subcmd__env__subcmd__review"
                ;;
            mm__subcmd__env,set)
                cmd="mm__subcmd__env__subcmd__set"
                ;;
            mm__subcmd__env,setup)
                cmd="mm__subcmd__env__subcmd__setup"
                ;;
            mm__subcmd__env,status)
                cmd="mm__subcmd__env__subcmd__status"
                ;;
            mm__subcmd__env,unset)
                cmd="mm__subcmd__env__subcmd__unset"
                ;;
            mm__subcmd__env__subcmd__default,help)
                cmd="mm__subcmd__env__subcmd__default__subcmd__help"
                ;;
            mm__subcmd__env__subcmd__default,set)
                cmd="mm__subcmd__env__subcmd__default__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__default,show)
                cmd="mm__subcmd__env__subcmd__default__subcmd__show"
                ;;
            mm__subcmd__env__subcmd__default,unset)
                cmd="mm__subcmd__env__subcmd__default__subcmd__unset"
                ;;
            mm__subcmd__env__subcmd__default__subcmd__help,help)
                cmd="mm__subcmd__env__subcmd__default__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__env__subcmd__default__subcmd__help,set)
                cmd="mm__subcmd__env__subcmd__default__subcmd__help__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__default__subcmd__help,show)
                cmd="mm__subcmd__env__subcmd__default__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__env__subcmd__default__subcmd__help,unset)
                cmd="mm__subcmd__env__subcmd__default__subcmd__help__subcmd__unset"
                ;;
            mm__subcmd__env__subcmd__delivery,help)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__help"
                ;;
            mm__subcmd__env__subcmd__delivery,set)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__delivery,show)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__show"
                ;;
            mm__subcmd__env__subcmd__delivery__subcmd__help,help)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__env__subcmd__delivery__subcmd__help,set)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__delivery__subcmd__help,show)
                cmd="mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__env__subcmd__help,check)
                cmd="mm__subcmd__env__subcmd__help__subcmd__check"
                ;;
            mm__subcmd__env__subcmd__help,default)
                cmd="mm__subcmd__env__subcmd__help__subcmd__default"
                ;;
            mm__subcmd__env__subcmd__help,delivery)
                cmd="mm__subcmd__env__subcmd__help__subcmd__delivery"
                ;;
            mm__subcmd__env__subcmd__help,diff)
                cmd="mm__subcmd__env__subcmd__help__subcmd__diff"
                ;;
            mm__subcmd__env__subcmd__help,discard)
                cmd="mm__subcmd__env__subcmd__help__subcmd__discard"
                ;;
            mm__subcmd__env__subcmd__help,explain)
                cmd="mm__subcmd__env__subcmd__help__subcmd__explain"
                ;;
            mm__subcmd__env__subcmd__help,help)
                cmd="mm__subcmd__env__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__env__subcmd__help,materialize)
                cmd="mm__subcmd__env__subcmd__help__subcmd__materialize"
                ;;
            mm__subcmd__env__subcmd__help,review)
                cmd="mm__subcmd__env__subcmd__help__subcmd__review"
                ;;
            mm__subcmd__env__subcmd__help,set)
                cmd="mm__subcmd__env__subcmd__help__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__help,setup)
                cmd="mm__subcmd__env__subcmd__help__subcmd__setup"
                ;;
            mm__subcmd__env__subcmd__help,status)
                cmd="mm__subcmd__env__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__env__subcmd__help,unset)
                cmd="mm__subcmd__env__subcmd__help__subcmd__unset"
                ;;
            mm__subcmd__env__subcmd__help__subcmd__default,set)
                cmd="mm__subcmd__env__subcmd__help__subcmd__default__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__help__subcmd__default,show)
                cmd="mm__subcmd__env__subcmd__help__subcmd__default__subcmd__show"
                ;;
            mm__subcmd__env__subcmd__help__subcmd__default,unset)
                cmd="mm__subcmd__env__subcmd__help__subcmd__default__subcmd__unset"
                ;;
            mm__subcmd__env__subcmd__help__subcmd__delivery,set)
                cmd="mm__subcmd__env__subcmd__help__subcmd__delivery__subcmd__set"
                ;;
            mm__subcmd__env__subcmd__help__subcmd__delivery,show)
                cmd="mm__subcmd__env__subcmd__help__subcmd__delivery__subcmd__show"
                ;;
            mm__subcmd__forward,help)
                cmd="mm__subcmd__forward__subcmd__help"
                ;;
            mm__subcmd__forward,list)
                cmd="mm__subcmd__forward__subcmd__list"
                ;;
            mm__subcmd__forward,remove)
                cmd="mm__subcmd__forward__subcmd__remove"
                ;;
            mm__subcmd__forward,save)
                cmd="mm__subcmd__forward__subcmd__save"
                ;;
            mm__subcmd__forward,up)
                cmd="mm__subcmd__forward__subcmd__up"
                ;;
            mm__subcmd__forward__subcmd__help,help)
                cmd="mm__subcmd__forward__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__forward__subcmd__help,list)
                cmd="mm__subcmd__forward__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__forward__subcmd__help,remove)
                cmd="mm__subcmd__forward__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__forward__subcmd__help,save)
                cmd="mm__subcmd__forward__subcmd__help__subcmd__save"
                ;;
            mm__subcmd__forward__subcmd__help,up)
                cmd="mm__subcmd__forward__subcmd__help__subcmd__up"
                ;;
            mm__subcmd__help,access)
                cmd="mm__subcmd__help__subcmd__access"
                ;;
            mm__subcmd__help,agent)
                cmd="mm__subcmd__help__subcmd__agent"
                ;;
            mm__subcmd__help,build)
                cmd="mm__subcmd__help__subcmd__build"
                ;;
            mm__subcmd__help,config)
                cmd="mm__subcmd__help__subcmd__config"
                ;;
            mm__subcmd__help,deploy)
                cmd="mm__subcmd__help__subcmd__deploy"
                ;;
            mm__subcmd__help,dev)
                cmd="mm__subcmd__help__subcmd__dev"
                ;;
            mm__subcmd__help,doctor)
                cmd="mm__subcmd__help__subcmd__doctor"
                ;;
            mm__subcmd__help,env)
                cmd="mm__subcmd__help__subcmd__env"
                ;;
            mm__subcmd__help,forward)
                cmd="mm__subcmd__help__subcmd__forward"
                ;;
            mm__subcmd__help,help)
                cmd="mm__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__help,history)
                cmd="mm__subcmd__help__subcmd__history"
                ;;
            mm__subcmd__help,ingress)
                cmd="mm__subcmd__help__subcmd__ingress"
                ;;
            mm__subcmd__help,init)
                cmd="mm__subcmd__help__subcmd__init"
                ;;
            mm__subcmd__help,job)
                cmd="mm__subcmd__help__subcmd__job"
                ;;
            mm__subcmd__help,key)
                cmd="mm__subcmd__help__subcmd__key"
                ;;
            mm__subcmd__help,mcp)
                cmd="mm__subcmd__help__subcmd__mcp"
                ;;
            mm__subcmd__help,secret)
                cmd="mm__subcmd__help__subcmd__secret"
                ;;
            mm__subcmd__help,service)
                cmd="mm__subcmd__help__subcmd__service"
                ;;
            mm__subcmd__help,target)
                cmd="mm__subcmd__help__subcmd__target"
                ;;
            mm__subcmd__help,team)
                cmd="mm__subcmd__help__subcmd__team"
                ;;
            mm__subcmd__help,update)
                cmd="mm__subcmd__help__subcmd__update"
                ;;
            mm__subcmd__help,workspace)
                cmd="mm__subcmd__help__subcmd__workspace"
                ;;
            mm__subcmd__help__subcmd__access,add)
                cmd="mm__subcmd__help__subcmd__access__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__access,attach)
                cmd="mm__subcmd__help__subcmd__access__subcmd__attach"
                ;;
            mm__subcmd__help__subcmd__access,detach)
                cmd="mm__subcmd__help__subcmd__access__subcmd__detach"
                ;;
            mm__subcmd__help__subcmd__access,remove)
                cmd="mm__subcmd__help__subcmd__access__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__access,session)
                cmd="mm__subcmd__help__subcmd__access__subcmd__session"
                ;;
            mm__subcmd__help__subcmd__access,show)
                cmd="mm__subcmd__help__subcmd__access__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__access,user)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__session,list)
                cmd="mm__subcmd__help__subcmd__access__subcmd__session__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__session,revoke)
                cmd="mm__subcmd__help__subcmd__access__subcmd__session__subcmd__revoke"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__session,rotate-key)
                cmd="mm__subcmd__help__subcmd__access__subcmd__session__subcmd__rotate__subcmd__key"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,add)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,disable)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__disable"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,enable)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__enable"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,list)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,password-reset)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__password__subcmd__reset"
                ;;
            mm__subcmd__help__subcmd__access__subcmd__user,remove)
                cmd="mm__subcmd__help__subcmd__access__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__agent,setup)
                cmd="mm__subcmd__help__subcmd__agent__subcmd__setup"
                ;;
            mm__subcmd__help__subcmd__agent,status)
                cmd="mm__subcmd__help__subcmd__agent__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__agent,trust)
                cmd="mm__subcmd__help__subcmd__agent__subcmd__trust"
                ;;
            mm__subcmd__help__subcmd__build,apply)
                cmd="mm__subcmd__help__subcmd__build__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__build,import)
                cmd="mm__subcmd__help__subcmd__build__subcmd__import"
                ;;
            mm__subcmd__help__subcmd__build,plan)
                cmd="mm__subcmd__help__subcmd__build__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__build,receipt)
                cmd="mm__subcmd__help__subcmd__build__subcmd__receipt"
                ;;
            mm__subcmd__help__subcmd__config,access)
                cmd="mm__subcmd__help__subcmd__config__subcmd__access"
                ;;
            mm__subcmd__help__subcmd__config,binding)
                cmd="mm__subcmd__help__subcmd__config__subcmd__binding"
                ;;
            mm__subcmd__help__subcmd__config,build)
                cmd="mm__subcmd__help__subcmd__config__subcmd__build"
                ;;
            mm__subcmd__help__subcmd__config,environment)
                cmd="mm__subcmd__help__subcmd__config__subcmd__environment"
                ;;
            mm__subcmd__help__subcmd__config,ingress)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress"
                ;;
            mm__subcmd__help__subcmd__config,job)
                cmd="mm__subcmd__help__subcmd__config__subcmd__job"
                ;;
            mm__subcmd__help__subcmd__config,join)
                cmd="mm__subcmd__help__subcmd__config__subcmd__join"
                ;;
            mm__subcmd__help__subcmd__config,module)
                cmd="mm__subcmd__help__subcmd__config__subcmd__module"
                ;;
            mm__subcmd__help__subcmd__config,mount)
                cmd="mm__subcmd__help__subcmd__config__subcmd__mount"
                ;;
            mm__subcmd__help__subcmd__config,schedule)
                cmd="mm__subcmd__help__subcmd__config__subcmd__schedule"
                ;;
            mm__subcmd__help__subcmd__config,schema)
                cmd="mm__subcmd__help__subcmd__config__subcmd__schema"
                ;;
            mm__subcmd__help__subcmd__config,secret)
                cmd="mm__subcmd__help__subcmd__config__subcmd__secret"
                ;;
            mm__subcmd__help__subcmd__config,service)
                cmd="mm__subcmd__help__subcmd__config__subcmd__service"
                ;;
            mm__subcmd__help__subcmd__config,sources)
                cmd="mm__subcmd__help__subcmd__config__subcmd__sources"
                ;;
            mm__subcmd__help__subcmd__config,split)
                cmd="mm__subcmd__help__subcmd__config__subcmd__split"
                ;;
            mm__subcmd__help__subcmd__config,stack)
                cmd="mm__subcmd__help__subcmd__config__subcmd__stack"
                ;;
            mm__subcmd__help__subcmd__config,user)
                cmd="mm__subcmd__help__subcmd__config__subcmd__user"
                ;;
            mm__subcmd__help__subcmd__config,validate)
                cmd="mm__subcmd__help__subcmd__config__subcmd__validate"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__access,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__access__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__access,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__access__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__access,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__access__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__binding,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__binding,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__binding,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__build,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__build__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__build,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__build__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__build,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__build__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__environment,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__environment,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__environment,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__ingress,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__ingress,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__ingress,explain)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__explain"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__ingress,list)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__ingress,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__job,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__job__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__job,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__job__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__job,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__job__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__module,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__module__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__module,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__module__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__module,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__module__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__mount,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__mount,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__mount,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__schedule,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__schedule,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__schedule,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__secret,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__secret,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__secret,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__service,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__service__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__service,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__service__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__service,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__service__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__stack,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__stack,compile)
                cmd="mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__compile"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__stack,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__stack,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__user,add)
                cmd="mm__subcmd__help__subcmd__config__subcmd__user__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__user,edit)
                cmd="mm__subcmd__help__subcmd__config__subcmd__user__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__config__subcmd__user,remove)
                cmd="mm__subcmd__help__subcmd__config__subcmd__user__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__deploy,apply)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__deploy,cancel)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__cancel"
                ;;
            mm__subcmd__help__subcmd__deploy,logs)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__logs"
                ;;
            mm__subcmd__help__subcmd__deploy,plan)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__deploy,redeploy)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__redeploy"
                ;;
            mm__subcmd__help__subcmd__deploy,rollback)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__rollback"
                ;;
            mm__subcmd__help__subcmd__deploy,status)
                cmd="mm__subcmd__help__subcmd__deploy__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__dev,dashboard)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__dashboard"
                ;;
            mm__subcmd__help__subcmd__dev,disable)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__disable"
                ;;
            mm__subcmd__help__subcmd__dev,down)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__down"
                ;;
            mm__subcmd__help__subcmd__dev,enable)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__enable"
                ;;
            mm__subcmd__help__subcmd__dev,mounts)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__mounts"
                ;;
            mm__subcmd__help__subcmd__dev,open)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__open"
                ;;
            mm__subcmd__help__subcmd__dev,services)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services"
                ;;
            mm__subcmd__help__subcmd__dev,setup)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__setup"
                ;;
            mm__subcmd__help__subcmd__dev,status)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__dev,tls)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__tls"
                ;;
            mm__subcmd__help__subcmd__dev,up)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__up"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__dashboard,serve)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__dashboard__subcmd__serve"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__mounts,configure)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__configure"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__mounts,disable)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__disable"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__mounts,plan)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,apply)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,plan)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,remove)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,repair)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__repair"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,status)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__services,upgrade)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__upgrade"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__setup,project)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__setup__subcmd__project"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__tls,repair)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__repair"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__tls,rotate)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__rotate"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__tls,status)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__dev__subcmd__tls,uninstall)
                cmd="mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__uninstall"
                ;;
            mm__subcmd__help__subcmd__doctor,recover-state)
                cmd="mm__subcmd__help__subcmd__doctor__subcmd__recover__subcmd__state"
                ;;
            mm__subcmd__help__subcmd__env,check)
                cmd="mm__subcmd__help__subcmd__env__subcmd__check"
                ;;
            mm__subcmd__help__subcmd__env,default)
                cmd="mm__subcmd__help__subcmd__env__subcmd__default"
                ;;
            mm__subcmd__help__subcmd__env,delivery)
                cmd="mm__subcmd__help__subcmd__env__subcmd__delivery"
                ;;
            mm__subcmd__help__subcmd__env,diff)
                cmd="mm__subcmd__help__subcmd__env__subcmd__diff"
                ;;
            mm__subcmd__help__subcmd__env,discard)
                cmd="mm__subcmd__help__subcmd__env__subcmd__discard"
                ;;
            mm__subcmd__help__subcmd__env,explain)
                cmd="mm__subcmd__help__subcmd__env__subcmd__explain"
                ;;
            mm__subcmd__help__subcmd__env,materialize)
                cmd="mm__subcmd__help__subcmd__env__subcmd__materialize"
                ;;
            mm__subcmd__help__subcmd__env,review)
                cmd="mm__subcmd__help__subcmd__env__subcmd__review"
                ;;
            mm__subcmd__help__subcmd__env,set)
                cmd="mm__subcmd__help__subcmd__env__subcmd__set"
                ;;
            mm__subcmd__help__subcmd__env,setup)
                cmd="mm__subcmd__help__subcmd__env__subcmd__setup"
                ;;
            mm__subcmd__help__subcmd__env,status)
                cmd="mm__subcmd__help__subcmd__env__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__env,unset)
                cmd="mm__subcmd__help__subcmd__env__subcmd__unset"
                ;;
            mm__subcmd__help__subcmd__env__subcmd__default,set)
                cmd="mm__subcmd__help__subcmd__env__subcmd__default__subcmd__set"
                ;;
            mm__subcmd__help__subcmd__env__subcmd__default,show)
                cmd="mm__subcmd__help__subcmd__env__subcmd__default__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__env__subcmd__default,unset)
                cmd="mm__subcmd__help__subcmd__env__subcmd__default__subcmd__unset"
                ;;
            mm__subcmd__help__subcmd__env__subcmd__delivery,set)
                cmd="mm__subcmd__help__subcmd__env__subcmd__delivery__subcmd__set"
                ;;
            mm__subcmd__help__subcmd__env__subcmd__delivery,show)
                cmd="mm__subcmd__help__subcmd__env__subcmd__delivery__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__forward,list)
                cmd="mm__subcmd__help__subcmd__forward__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__forward,remove)
                cmd="mm__subcmd__help__subcmd__forward__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__forward,save)
                cmd="mm__subcmd__help__subcmd__forward__subcmd__save"
                ;;
            mm__subcmd__help__subcmd__forward,up)
                cmd="mm__subcmd__help__subcmd__forward__subcmd__up"
                ;;
            mm__subcmd__help__subcmd__history,show)
                cmd="mm__subcmd__help__subcmd__history__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__ingress,restart)
                cmd="mm__subcmd__help__subcmd__ingress__subcmd__restart"
                ;;
            mm__subcmd__help__subcmd__ingress,start)
                cmd="mm__subcmd__help__subcmd__ingress__subcmd__start"
                ;;
            mm__subcmd__help__subcmd__ingress,status)
                cmd="mm__subcmd__help__subcmd__ingress__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__ingress,stop)
                cmd="mm__subcmd__help__subcmd__ingress__subcmd__stop"
                ;;
            mm__subcmd__help__subcmd__job,cancel)
                cmd="mm__subcmd__help__subcmd__job__subcmd__cancel"
                ;;
            mm__subcmd__help__subcmd__job,history)
                cmd="mm__subcmd__help__subcmd__job__subcmd__history"
                ;;
            mm__subcmd__help__subcmd__job,list)
                cmd="mm__subcmd__help__subcmd__job__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__job,retry)
                cmd="mm__subcmd__help__subcmd__job__subcmd__retry"
                ;;
            mm__subcmd__help__subcmd__job,run)
                cmd="mm__subcmd__help__subcmd__job__subcmd__run"
                ;;
            mm__subcmd__help__subcmd__job,schedule)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule"
                ;;
            mm__subcmd__help__subcmd__job,status)
                cmd="mm__subcmd__help__subcmd__job__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,daemon)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__daemon"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,disable)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__disable"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,enable)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__enable"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,history)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__history"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,list)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,next)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__next"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,reconcile)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__reconcile"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,run-now)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__run__subcmd__now"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,show)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__job__subcmd__schedule,tick)
                cmd="mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__tick"
                ;;
            mm__subcmd__help__subcmd__key,create)
                cmd="mm__subcmd__help__subcmd__key__subcmd__create"
                ;;
            mm__subcmd__help__subcmd__key,protect)
                cmd="mm__subcmd__help__subcmd__key__subcmd__protect"
                ;;
            mm__subcmd__help__subcmd__key,remove)
                cmd="mm__subcmd__help__subcmd__key__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__key,rotate)
                cmd="mm__subcmd__help__subcmd__key__subcmd__rotate"
                ;;
            mm__subcmd__help__subcmd__key,show)
                cmd="mm__subcmd__help__subcmd__key__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__mcp,serve)
                cmd="mm__subcmd__help__subcmd__mcp__subcmd__serve"
                ;;
            mm__subcmd__help__subcmd__secret,discard)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__discard"
                ;;
            mm__subcmd__help__subcmd__secret,generate)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__generate"
                ;;
            mm__subcmd__help__subcmd__secret,init)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__init"
                ;;
            mm__subcmd__help__subcmd__secret,migrate)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__migrate"
                ;;
            mm__subcmd__help__subcmd__secret,provider)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider"
                ;;
            mm__subcmd__help__subcmd__secret,providers)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__providers"
                ;;
            mm__subcmd__help__subcmd__secret,rotate)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__rotate"
                ;;
            mm__subcmd__help__subcmd__secret,rotation)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__rotation"
                ;;
            mm__subcmd__help__subcmd__secret,set)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__set"
                ;;
            mm__subcmd__help__subcmd__secret,status)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__secret,unset)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__unset"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,add)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,apply)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,backup)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__backup"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,edit)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__edit"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,list)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,plan)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,recovery)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,remove)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,restore)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__restore"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,show)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,snapshot)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__snapshot"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,status)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider,test)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__test"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery,export)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__export"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery,import)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__import"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__rotation,apply)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__rotation__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__secret__subcmd__rotation,plan)
                cmd="mm__subcmd__help__subcmd__secret__subcmd__rotation__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__service,exec)
                cmd="mm__subcmd__help__subcmd__service__subcmd__exec"
                ;;
            mm__subcmd__help__subcmd__service,logs)
                cmd="mm__subcmd__help__subcmd__service__subcmd__logs"
                ;;
            mm__subcmd__help__subcmd__service,restart)
                cmd="mm__subcmd__help__subcmd__service__subcmd__restart"
                ;;
            mm__subcmd__help__subcmd__service,shell)
                cmd="mm__subcmd__help__subcmd__service__subcmd__shell"
                ;;
            mm__subcmd__help__subcmd__service,status)
                cmd="mm__subcmd__help__subcmd__service__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__target,access)
                cmd="mm__subcmd__help__subcmd__target__subcmd__access"
                ;;
            mm__subcmd__help__subcmd__target,add)
                cmd="mm__subcmd__help__subcmd__target__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__target,connections)
                cmd="mm__subcmd__help__subcmd__target__subcmd__connections"
                ;;
            mm__subcmd__help__subcmd__target,ingress)
                cmd="mm__subcmd__help__subcmd__target__subcmd__ingress"
                ;;
            mm__subcmd__help__subcmd__target,list)
                cmd="mm__subcmd__help__subcmd__target__subcmd__list"
                ;;
            mm__subcmd__help__subcmd__target,local)
                cmd="mm__subcmd__help__subcmd__target__subcmd__local"
                ;;
            mm__subcmd__help__subcmd__target,mesh)
                cmd="mm__subcmd__help__subcmd__target__subcmd__mesh"
                ;;
            mm__subcmd__help__subcmd__target,nodes)
                cmd="mm__subcmd__help__subcmd__target__subcmd__nodes"
                ;;
            mm__subcmd__help__subcmd__target,preference)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference"
                ;;
            mm__subcmd__help__subcmd__target,remove)
                cmd="mm__subcmd__help__subcmd__target__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__target,select)
                cmd="mm__subcmd__help__subcmd__target__subcmd__select"
                ;;
            mm__subcmd__help__subcmd__target,setup)
                cmd="mm__subcmd__help__subcmd__target__subcmd__setup"
                ;;
            mm__subcmd__help__subcmd__target,show)
                cmd="mm__subcmd__help__subcmd__target__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__target,storage)
                cmd="mm__subcmd__help__subcmd__target__subcmd__storage"
                ;;
            mm__subcmd__help__subcmd__target,trust)
                cmd="mm__subcmd__help__subcmd__target__subcmd__trust"
                ;;
            mm__subcmd__help__subcmd__target,update)
                cmd="mm__subcmd__help__subcmd__target__subcmd__update"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__access,status)
                cmd="mm__subcmd__help__subcmd__target__subcmd__access__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__ingress,configure)
                cmd="mm__subcmd__help__subcmd__target__subcmd__ingress__subcmd__configure"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__ingress,plan)
                cmd="mm__subcmd__help__subcmd__target__subcmd__ingress__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__mesh,rotate)
                cmd="mm__subcmd__help__subcmd__target__subcmd__mesh__subcmd__rotate"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__mesh,status)
                cmd="mm__subcmd__help__subcmd__target__subcmd__mesh__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,set-address)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__address"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,set-runtime)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__runtime"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,set-target)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__target"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,show)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,unset-address)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__address"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,unset-runtime)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__runtime"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__preference,unset-target)
                cmd="mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__target"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__setup,apply)
                cmd="mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__apply"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__setup,deploy-key)
                cmd="mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__deploy__subcmd__key"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__setup,plan)
                cmd="mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__plan"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__setup,record)
                cmd="mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__record"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__storage,delete)
                cmd="mm__subcmd__help__subcmd__target__subcmd__storage__subcmd__delete"
                ;;
            mm__subcmd__help__subcmd__target__subcmd__storage,status)
                cmd="mm__subcmd__help__subcmd__target__subcmd__storage__subcmd__status"
                ;;
            mm__subcmd__help__subcmd__team,add)
                cmd="mm__subcmd__help__subcmd__team__subcmd__add"
                ;;
            mm__subcmd__help__subcmd__team,join)
                cmd="mm__subcmd__help__subcmd__team__subcmd__join"
                ;;
            mm__subcmd__help__subcmd__team,remove)
                cmd="mm__subcmd__help__subcmd__team__subcmd__remove"
                ;;
            mm__subcmd__help__subcmd__team,show)
                cmd="mm__subcmd__help__subcmd__team__subcmd__show"
                ;;
            mm__subcmd__help__subcmd__team,sync)
                cmd="mm__subcmd__help__subcmd__team__subcmd__sync"
                ;;
            mm__subcmd__help__subcmd__team,whoami)
                cmd="mm__subcmd__help__subcmd__team__subcmd__whoami"
                ;;
            mm__subcmd__help__subcmd__workspace,forget)
                cmd="mm__subcmd__help__subcmd__workspace__subcmd__forget"
                ;;
            mm__subcmd__help__subcmd__workspace,list)
                cmd="mm__subcmd__help__subcmd__workspace__subcmd__list"
                ;;
            mm__subcmd__history,help)
                cmd="mm__subcmd__history__subcmd__help"
                ;;
            mm__subcmd__history,show)
                cmd="mm__subcmd__history__subcmd__show"
                ;;
            mm__subcmd__history__subcmd__help,help)
                cmd="mm__subcmd__history__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__history__subcmd__help,show)
                cmd="mm__subcmd__history__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__ingress,help)
                cmd="mm__subcmd__ingress__subcmd__help"
                ;;
            mm__subcmd__ingress,restart)
                cmd="mm__subcmd__ingress__subcmd__restart"
                ;;
            mm__subcmd__ingress,start)
                cmd="mm__subcmd__ingress__subcmd__start"
                ;;
            mm__subcmd__ingress,status)
                cmd="mm__subcmd__ingress__subcmd__status"
                ;;
            mm__subcmd__ingress,stop)
                cmd="mm__subcmd__ingress__subcmd__stop"
                ;;
            mm__subcmd__ingress__subcmd__help,help)
                cmd="mm__subcmd__ingress__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__ingress__subcmd__help,restart)
                cmd="mm__subcmd__ingress__subcmd__help__subcmd__restart"
                ;;
            mm__subcmd__ingress__subcmd__help,start)
                cmd="mm__subcmd__ingress__subcmd__help__subcmd__start"
                ;;
            mm__subcmd__ingress__subcmd__help,status)
                cmd="mm__subcmd__ingress__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__ingress__subcmd__help,stop)
                cmd="mm__subcmd__ingress__subcmd__help__subcmd__stop"
                ;;
            mm__subcmd__job,cancel)
                cmd="mm__subcmd__job__subcmd__cancel"
                ;;
            mm__subcmd__job,help)
                cmd="mm__subcmd__job__subcmd__help"
                ;;
            mm__subcmd__job,history)
                cmd="mm__subcmd__job__subcmd__history"
                ;;
            mm__subcmd__job,list)
                cmd="mm__subcmd__job__subcmd__list"
                ;;
            mm__subcmd__job,retry)
                cmd="mm__subcmd__job__subcmd__retry"
                ;;
            mm__subcmd__job,run)
                cmd="mm__subcmd__job__subcmd__run"
                ;;
            mm__subcmd__job,schedule)
                cmd="mm__subcmd__job__subcmd__schedule"
                ;;
            mm__subcmd__job,status)
                cmd="mm__subcmd__job__subcmd__status"
                ;;
            mm__subcmd__job__subcmd__help,cancel)
                cmd="mm__subcmd__job__subcmd__help__subcmd__cancel"
                ;;
            mm__subcmd__job__subcmd__help,help)
                cmd="mm__subcmd__job__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__job__subcmd__help,history)
                cmd="mm__subcmd__job__subcmd__help__subcmd__history"
                ;;
            mm__subcmd__job__subcmd__help,list)
                cmd="mm__subcmd__job__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__job__subcmd__help,retry)
                cmd="mm__subcmd__job__subcmd__help__subcmd__retry"
                ;;
            mm__subcmd__job__subcmd__help,run)
                cmd="mm__subcmd__job__subcmd__help__subcmd__run"
                ;;
            mm__subcmd__job__subcmd__help,schedule)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule"
                ;;
            mm__subcmd__job__subcmd__help,status)
                cmd="mm__subcmd__job__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,daemon)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__daemon"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,disable)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__disable"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,enable)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__enable"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,history)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__history"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,list)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__list"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,next)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__next"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,reconcile)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__reconcile"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,run-now)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__run__subcmd__now"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,show)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__show"
                ;;
            mm__subcmd__job__subcmd__help__subcmd__schedule,tick)
                cmd="mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__tick"
                ;;
            mm__subcmd__job__subcmd__schedule,daemon)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__daemon"
                ;;
            mm__subcmd__job__subcmd__schedule,disable)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__disable"
                ;;
            mm__subcmd__job__subcmd__schedule,enable)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__enable"
                ;;
            mm__subcmd__job__subcmd__schedule,help)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help"
                ;;
            mm__subcmd__job__subcmd__schedule,history)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__history"
                ;;
            mm__subcmd__job__subcmd__schedule,list)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__list"
                ;;
            mm__subcmd__job__subcmd__schedule,next)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__next"
                ;;
            mm__subcmd__job__subcmd__schedule,reconcile)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__reconcile"
                ;;
            mm__subcmd__job__subcmd__schedule,run-now)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__run__subcmd__now"
                ;;
            mm__subcmd__job__subcmd__schedule,show)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__show"
                ;;
            mm__subcmd__job__subcmd__schedule,tick)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__tick"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,daemon)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__daemon"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,disable)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__disable"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,enable)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__enable"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,help)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,history)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__history"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,list)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,next)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__next"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,reconcile)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__reconcile"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,run-now)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__run__subcmd__now"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,show)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__job__subcmd__schedule__subcmd__help,tick)
                cmd="mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__tick"
                ;;
            mm__subcmd__key,create)
                cmd="mm__subcmd__key__subcmd__create"
                ;;
            mm__subcmd__key,help)
                cmd="mm__subcmd__key__subcmd__help"
                ;;
            mm__subcmd__key,protect)
                cmd="mm__subcmd__key__subcmd__protect"
                ;;
            mm__subcmd__key,remove)
                cmd="mm__subcmd__key__subcmd__remove"
                ;;
            mm__subcmd__key,rotate)
                cmd="mm__subcmd__key__subcmd__rotate"
                ;;
            mm__subcmd__key,show)
                cmd="mm__subcmd__key__subcmd__show"
                ;;
            mm__subcmd__key__subcmd__help,create)
                cmd="mm__subcmd__key__subcmd__help__subcmd__create"
                ;;
            mm__subcmd__key__subcmd__help,help)
                cmd="mm__subcmd__key__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__key__subcmd__help,protect)
                cmd="mm__subcmd__key__subcmd__help__subcmd__protect"
                ;;
            mm__subcmd__key__subcmd__help,remove)
                cmd="mm__subcmd__key__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__key__subcmd__help,rotate)
                cmd="mm__subcmd__key__subcmd__help__subcmd__rotate"
                ;;
            mm__subcmd__key__subcmd__help,show)
                cmd="mm__subcmd__key__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__mcp,help)
                cmd="mm__subcmd__mcp__subcmd__help"
                ;;
            mm__subcmd__mcp,serve)
                cmd="mm__subcmd__mcp__subcmd__serve"
                ;;
            mm__subcmd__mcp__subcmd__help,help)
                cmd="mm__subcmd__mcp__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__mcp__subcmd__help,serve)
                cmd="mm__subcmd__mcp__subcmd__help__subcmd__serve"
                ;;
            mm__subcmd__secret,discard)
                cmd="mm__subcmd__secret__subcmd__discard"
                ;;
            mm__subcmd__secret,generate)
                cmd="mm__subcmd__secret__subcmd__generate"
                ;;
            mm__subcmd__secret,help)
                cmd="mm__subcmd__secret__subcmd__help"
                ;;
            mm__subcmd__secret,init)
                cmd="mm__subcmd__secret__subcmd__init"
                ;;
            mm__subcmd__secret,migrate)
                cmd="mm__subcmd__secret__subcmd__migrate"
                ;;
            mm__subcmd__secret,provider)
                cmd="mm__subcmd__secret__subcmd__provider"
                ;;
            mm__subcmd__secret,providers)
                cmd="mm__subcmd__secret__subcmd__providers"
                ;;
            mm__subcmd__secret,rotate)
                cmd="mm__subcmd__secret__subcmd__rotate"
                ;;
            mm__subcmd__secret,rotation)
                cmd="mm__subcmd__secret__subcmd__rotation"
                ;;
            mm__subcmd__secret,set)
                cmd="mm__subcmd__secret__subcmd__set"
                ;;
            mm__subcmd__secret,status)
                cmd="mm__subcmd__secret__subcmd__status"
                ;;
            mm__subcmd__secret,unset)
                cmd="mm__subcmd__secret__subcmd__unset"
                ;;
            mm__subcmd__secret__subcmd__help,discard)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__discard"
                ;;
            mm__subcmd__secret__subcmd__help,generate)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__generate"
                ;;
            mm__subcmd__secret__subcmd__help,help)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__help,init)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__init"
                ;;
            mm__subcmd__secret__subcmd__help,migrate)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__migrate"
                ;;
            mm__subcmd__secret__subcmd__help,provider)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider"
                ;;
            mm__subcmd__secret__subcmd__help,providers)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__providers"
                ;;
            mm__subcmd__secret__subcmd__help,rotate)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__rotate"
                ;;
            mm__subcmd__secret__subcmd__help,rotation)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__rotation"
                ;;
            mm__subcmd__secret__subcmd__help,set)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__set"
                ;;
            mm__subcmd__secret__subcmd__help,status)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__secret__subcmd__help,unset)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__unset"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,add)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__add"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,apply)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,backup)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__backup"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,edit)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__edit"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,list)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__list"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,plan)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__plan"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,recovery)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,remove)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__remove"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,restore)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__restore"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,show)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__show"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,snapshot)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__snapshot"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,status)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__status"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider,test)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__test"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery,export)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery__subcmd__export"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery,import)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery__subcmd__import"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__rotation,apply)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__rotation__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__help__subcmd__rotation,plan)
                cmd="mm__subcmd__secret__subcmd__help__subcmd__rotation__subcmd__plan"
                ;;
            mm__subcmd__secret__subcmd__provider,add)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__add"
                ;;
            mm__subcmd__secret__subcmd__provider,apply)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__provider,backup)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__backup"
                ;;
            mm__subcmd__secret__subcmd__provider,edit)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__edit"
                ;;
            mm__subcmd__secret__subcmd__provider,help)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__provider,list)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__list"
                ;;
            mm__subcmd__secret__subcmd__provider,plan)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__plan"
                ;;
            mm__subcmd__secret__subcmd__provider,recovery)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery"
                ;;
            mm__subcmd__secret__subcmd__provider,remove)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__remove"
                ;;
            mm__subcmd__secret__subcmd__provider,restore)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__restore"
                ;;
            mm__subcmd__secret__subcmd__provider,show)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__show"
                ;;
            mm__subcmd__secret__subcmd__provider,snapshot)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__snapshot"
                ;;
            mm__subcmd__secret__subcmd__provider,status)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__status"
                ;;
            mm__subcmd__secret__subcmd__provider,test)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__test"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,add)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,apply)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,backup)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__backup"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,edit)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__edit"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,help)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,list)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,plan)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,recovery)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,remove)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,restore)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__restore"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,show)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,snapshot)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__snapshot"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,status)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help,test)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__test"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery,export)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery__subcmd__export"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery,import)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery__subcmd__import"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery,export)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__export"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery,help)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery,import)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__import"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help,export)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__export"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help,help)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help,import)
                cmd="mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__import"
                ;;
            mm__subcmd__secret__subcmd__rotation,apply)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__rotation,help)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__rotation,plan)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__plan"
                ;;
            mm__subcmd__secret__subcmd__rotation__subcmd__help,apply)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__secret__subcmd__rotation__subcmd__help,help)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__secret__subcmd__rotation__subcmd__help,plan)
                cmd="mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__service,exec)
                cmd="mm__subcmd__service__subcmd__exec"
                ;;
            mm__subcmd__service,help)
                cmd="mm__subcmd__service__subcmd__help"
                ;;
            mm__subcmd__service,logs)
                cmd="mm__subcmd__service__subcmd__logs"
                ;;
            mm__subcmd__service,restart)
                cmd="mm__subcmd__service__subcmd__restart"
                ;;
            mm__subcmd__service,shell)
                cmd="mm__subcmd__service__subcmd__shell"
                ;;
            mm__subcmd__service,status)
                cmd="mm__subcmd__service__subcmd__status"
                ;;
            mm__subcmd__service__subcmd__help,exec)
                cmd="mm__subcmd__service__subcmd__help__subcmd__exec"
                ;;
            mm__subcmd__service__subcmd__help,help)
                cmd="mm__subcmd__service__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__service__subcmd__help,logs)
                cmd="mm__subcmd__service__subcmd__help__subcmd__logs"
                ;;
            mm__subcmd__service__subcmd__help,restart)
                cmd="mm__subcmd__service__subcmd__help__subcmd__restart"
                ;;
            mm__subcmd__service__subcmd__help,shell)
                cmd="mm__subcmd__service__subcmd__help__subcmd__shell"
                ;;
            mm__subcmd__service__subcmd__help,status)
                cmd="mm__subcmd__service__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__target,access)
                cmd="mm__subcmd__target__subcmd__access"
                ;;
            mm__subcmd__target,add)
                cmd="mm__subcmd__target__subcmd__add"
                ;;
            mm__subcmd__target,connections)
                cmd="mm__subcmd__target__subcmd__connections"
                ;;
            mm__subcmd__target,help)
                cmd="mm__subcmd__target__subcmd__help"
                ;;
            mm__subcmd__target,ingress)
                cmd="mm__subcmd__target__subcmd__ingress"
                ;;
            mm__subcmd__target,list)
                cmd="mm__subcmd__target__subcmd__list"
                ;;
            mm__subcmd__target,local)
                cmd="mm__subcmd__target__subcmd__local"
                ;;
            mm__subcmd__target,mesh)
                cmd="mm__subcmd__target__subcmd__mesh"
                ;;
            mm__subcmd__target,nodes)
                cmd="mm__subcmd__target__subcmd__nodes"
                ;;
            mm__subcmd__target,preference)
                cmd="mm__subcmd__target__subcmd__preference"
                ;;
            mm__subcmd__target,remove)
                cmd="mm__subcmd__target__subcmd__remove"
                ;;
            mm__subcmd__target,select)
                cmd="mm__subcmd__target__subcmd__select"
                ;;
            mm__subcmd__target,setup)
                cmd="mm__subcmd__target__subcmd__setup"
                ;;
            mm__subcmd__target,show)
                cmd="mm__subcmd__target__subcmd__show"
                ;;
            mm__subcmd__target,storage)
                cmd="mm__subcmd__target__subcmd__storage"
                ;;
            mm__subcmd__target,trust)
                cmd="mm__subcmd__target__subcmd__trust"
                ;;
            mm__subcmd__target,update)
                cmd="mm__subcmd__target__subcmd__update"
                ;;
            mm__subcmd__target__subcmd__access,help)
                cmd="mm__subcmd__target__subcmd__access__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__access,status)
                cmd="mm__subcmd__target__subcmd__access__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__access__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__access__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__access__subcmd__help,status)
                cmd="mm__subcmd__target__subcmd__access__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__help,access)
                cmd="mm__subcmd__target__subcmd__help__subcmd__access"
                ;;
            mm__subcmd__target__subcmd__help,add)
                cmd="mm__subcmd__target__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__target__subcmd__help,connections)
                cmd="mm__subcmd__target__subcmd__help__subcmd__connections"
                ;;
            mm__subcmd__target__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__help,ingress)
                cmd="mm__subcmd__target__subcmd__help__subcmd__ingress"
                ;;
            mm__subcmd__target__subcmd__help,list)
                cmd="mm__subcmd__target__subcmd__help__subcmd__list"
                ;;
            mm__subcmd__target__subcmd__help,local)
                cmd="mm__subcmd__target__subcmd__help__subcmd__local"
                ;;
            mm__subcmd__target__subcmd__help,mesh)
                cmd="mm__subcmd__target__subcmd__help__subcmd__mesh"
                ;;
            mm__subcmd__target__subcmd__help,nodes)
                cmd="mm__subcmd__target__subcmd__help__subcmd__nodes"
                ;;
            mm__subcmd__target__subcmd__help,preference)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference"
                ;;
            mm__subcmd__target__subcmd__help,remove)
                cmd="mm__subcmd__target__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__target__subcmd__help,select)
                cmd="mm__subcmd__target__subcmd__help__subcmd__select"
                ;;
            mm__subcmd__target__subcmd__help,setup)
                cmd="mm__subcmd__target__subcmd__help__subcmd__setup"
                ;;
            mm__subcmd__target__subcmd__help,show)
                cmd="mm__subcmd__target__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__target__subcmd__help,storage)
                cmd="mm__subcmd__target__subcmd__help__subcmd__storage"
                ;;
            mm__subcmd__target__subcmd__help,trust)
                cmd="mm__subcmd__target__subcmd__help__subcmd__trust"
                ;;
            mm__subcmd__target__subcmd__help,update)
                cmd="mm__subcmd__target__subcmd__help__subcmd__update"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__access,status)
                cmd="mm__subcmd__target__subcmd__help__subcmd__access__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__ingress,configure)
                cmd="mm__subcmd__target__subcmd__help__subcmd__ingress__subcmd__configure"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__ingress,plan)
                cmd="mm__subcmd__target__subcmd__help__subcmd__ingress__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__mesh,rotate)
                cmd="mm__subcmd__target__subcmd__help__subcmd__mesh__subcmd__rotate"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__mesh,status)
                cmd="mm__subcmd__target__subcmd__help__subcmd__mesh__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,set-address)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,set-runtime)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,set-target)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,show)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__show"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,unset-address)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,unset-runtime)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__preference,unset-target)
                cmd="mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__setup,apply)
                cmd="mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__apply"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__setup,deploy-key)
                cmd="mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__deploy__subcmd__key"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__setup,plan)
                cmd="mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__setup,record)
                cmd="mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__record"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__storage,delete)
                cmd="mm__subcmd__target__subcmd__help__subcmd__storage__subcmd__delete"
                ;;
            mm__subcmd__target__subcmd__help__subcmd__storage,status)
                cmd="mm__subcmd__target__subcmd__help__subcmd__storage__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__ingress,configure)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__configure"
                ;;
            mm__subcmd__target__subcmd__ingress,help)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__ingress,plan)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__ingress__subcmd__help,configure)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__configure"
                ;;
            mm__subcmd__target__subcmd__ingress__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__ingress__subcmd__help,plan)
                cmd="mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__mesh,help)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__mesh,rotate)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__rotate"
                ;;
            mm__subcmd__target__subcmd__mesh,status)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__mesh__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__mesh__subcmd__help,rotate)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__rotate"
                ;;
            mm__subcmd__target__subcmd__mesh__subcmd__help,status)
                cmd="mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__preference,help)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__preference,set-address)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__preference,set-runtime)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__preference,set-target)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__preference,show)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__show"
                ;;
            mm__subcmd__target__subcmd__preference,unset-address)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__preference,unset-runtime)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__preference,unset-target)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,set-address)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,set-runtime)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,set-target)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,show)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,unset-address)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__address"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,unset-runtime)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__runtime"
                ;;
            mm__subcmd__target__subcmd__preference__subcmd__help,unset-target)
                cmd="mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__target"
                ;;
            mm__subcmd__target__subcmd__setup,apply)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__apply"
                ;;
            mm__subcmd__target__subcmd__setup,deploy-key)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__deploy__subcmd__key"
                ;;
            mm__subcmd__target__subcmd__setup,help)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__setup,plan)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__setup,record)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__record"
                ;;
            mm__subcmd__target__subcmd__setup__subcmd__help,apply)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__apply"
                ;;
            mm__subcmd__target__subcmd__setup__subcmd__help,deploy-key)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__deploy__subcmd__key"
                ;;
            mm__subcmd__target__subcmd__setup__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__setup__subcmd__help,plan)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__plan"
                ;;
            mm__subcmd__target__subcmd__setup__subcmd__help,record)
                cmd="mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__record"
                ;;
            mm__subcmd__target__subcmd__storage,delete)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__delete"
                ;;
            mm__subcmd__target__subcmd__storage,help)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__storage,status)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__status"
                ;;
            mm__subcmd__target__subcmd__storage__subcmd__help,delete)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__delete"
                ;;
            mm__subcmd__target__subcmd__storage__subcmd__help,help)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__target__subcmd__storage__subcmd__help,status)
                cmd="mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__status"
                ;;
            mm__subcmd__team,add)
                cmd="mm__subcmd__team__subcmd__add"
                ;;
            mm__subcmd__team,help)
                cmd="mm__subcmd__team__subcmd__help"
                ;;
            mm__subcmd__team,join)
                cmd="mm__subcmd__team__subcmd__join"
                ;;
            mm__subcmd__team,remove)
                cmd="mm__subcmd__team__subcmd__remove"
                ;;
            mm__subcmd__team,show)
                cmd="mm__subcmd__team__subcmd__show"
                ;;
            mm__subcmd__team,sync)
                cmd="mm__subcmd__team__subcmd__sync"
                ;;
            mm__subcmd__team,whoami)
                cmd="mm__subcmd__team__subcmd__whoami"
                ;;
            mm__subcmd__team__subcmd__help,add)
                cmd="mm__subcmd__team__subcmd__help__subcmd__add"
                ;;
            mm__subcmd__team__subcmd__help,help)
                cmd="mm__subcmd__team__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__team__subcmd__help,join)
                cmd="mm__subcmd__team__subcmd__help__subcmd__join"
                ;;
            mm__subcmd__team__subcmd__help,remove)
                cmd="mm__subcmd__team__subcmd__help__subcmd__remove"
                ;;
            mm__subcmd__team__subcmd__help,show)
                cmd="mm__subcmd__team__subcmd__help__subcmd__show"
                ;;
            mm__subcmd__team__subcmd__help,sync)
                cmd="mm__subcmd__team__subcmd__help__subcmd__sync"
                ;;
            mm__subcmd__team__subcmd__help,whoami)
                cmd="mm__subcmd__team__subcmd__help__subcmd__whoami"
                ;;
            mm__subcmd__workspace,forget)
                cmd="mm__subcmd__workspace__subcmd__forget"
                ;;
            mm__subcmd__workspace,help)
                cmd="mm__subcmd__workspace__subcmd__help"
                ;;
            mm__subcmd__workspace,list)
                cmd="mm__subcmd__workspace__subcmd__list"
                ;;
            mm__subcmd__workspace__subcmd__help,forget)
                cmd="mm__subcmd__workspace__subcmd__help__subcmd__forget"
                ;;
            mm__subcmd__workspace__subcmd__help,help)
                cmd="mm__subcmd__workspace__subcmd__help__subcmd__help"
                ;;
            mm__subcmd__workspace__subcmd__help,list)
                cmd="mm__subcmd__workspace__subcmd__help__subcmd__list"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        mm)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace update mcp help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add remove show attach detach user session help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__add)
            opts="-e -h -V --mode --session-days --secret-provider --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --mode)
                    COMPREPLY=($(compgen -W "any all" -- "${cur}"))
                    return 0
                    ;;
                --session-days)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --secret-provider)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__attach)
            opts="-e -h -V --target-id --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --target-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__detach)
            opts="-e -h -V --target-id --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --target-id)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help)
            opts="add remove show attach detach user session help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__attach)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__detach)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__session)
            opts="list revoke rotate-key"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__session__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__session__subcmd__revoke)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__session__subcmd__rotate__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user)
            opts="add disable enable password-reset remove list"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__password__subcmd__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__help__subcmd__user__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__remove)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version list revoke rotate-key help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__help)
            opts="list revoke rotate-key help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__help__subcmd__revoke)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__help__subcmd__rotate__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__revoke)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__session__subcmd__rotate__subcmd__key)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add disable enable password-reset remove list help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__add)
            opts="-e -h -V --stdin --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__disable)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__enable)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help)
            opts="add disable enable password-reset remove list help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__password__subcmd__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__password__subcmd__reset)
            opts="-e -h -V --stdin --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__access__subcmd__user__subcmd__remove)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status trust setup help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__help)
            opts="status trust setup help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__help__subcmd__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__help__subcmd__trust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__setup)
            opts="-e -h -V --client --user --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --client)
                    COMPREPLY=($(compgen -W "claude-code opencode codex cursor" -- "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__agent__subcmd__trust)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build)
            opts="-e -h -V --publish --all --yes --root --environment --runtime --target --mm-home --output --help --version plan apply receipt import help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__apply)
            opts="-e -h -V --plan --publish --all --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help)
            opts="plan apply receipt import help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__help__subcmd__receipt)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__import)
            opts="-e -h -V --cache-only --plan --publish --all --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__plan)
            opts="-e -h -V --publish --all --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__build__subcmd__receipt)
            opts="-e -h -V --publish --all --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__access__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__binding__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__build__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__environment__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help)
            opts="validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__access)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__access__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__access__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__access__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__binding)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__binding__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__build)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__build__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__build__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__build__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__environment)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__environment__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress)
            opts="add edit remove list explain"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__ingress__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__job)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__job__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__job__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__job__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__join)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__module)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__module__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__module__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__module__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__mount)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__mount__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__schedule)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__schedule__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__schema)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__secret)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__secret__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__service)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__service__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__service__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__service__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__sources)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__split)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__stack)
            opts="add edit remove compile"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__compile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__stack__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__user)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__user__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__user__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__user__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__help__subcmd__validate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove list explain help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__explain)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help)
            opts="add edit remove list explain help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__ingress__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__job__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__join)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version project default-environment target runtime build networks stacks modules providers secrets bindings environment-delivery environments ingress development-mounts development-mail jobs schedules gates users agents targets registries hardening release-inputs"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__module__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__mount__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schedule__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__schema)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version project default-environment target runtime build networks stacks modules providers secrets bindings environment-delivery environments ingress development-mounts development-mail jobs schedules gates users agents targets registries hardening release-inputs"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__secret__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__service__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__sources)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__split)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version project default-environment target runtime build networks stacks modules providers secrets bindings environment-delivery environments ingress development-mounts development-mail jobs schedules gates users agents targets registries hardening release-inputs"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove compile help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__compile)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help)
            opts="add edit remove compile help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__compile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__stack__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__add)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__edit)
            opts="-e -h -V --parent --definition --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --definition)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__help)
            opts="add edit remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__user__subcmd__remove)
            opts="-e -h -V --parent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --parent)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__config__subcmd__validate)
            opts="-e -h -V --for-remote --host-policy --strict --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --host-policy)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version plan apply rollback redeploy status logs cancel help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__apply)
            opts="-e -h -V --plan --receipt --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --receipt)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__cancel)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help)
            opts="plan apply rollback redeploy status logs cancel help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__cancel)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__logs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__redeploy)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__rollback)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__logs)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__plan)
            opts="-e -h -V --commit --receipt --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --commit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --receipt)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__redeploy)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__rollback)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__deploy__subcmd__status)
            opts="-e -h -V --yes --nodes --release --release-input --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --release-input)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version setup up enable disable down status tls services dashboard mounts open help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__dashboard)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version serve help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__dashboard__subcmd__help)
            opts="serve help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__dashboard__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__dashboard__subcmd__help__subcmd__serve)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__dashboard__subcmd__serve)
            opts="-e -h -V --listen --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --listen)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__disable)
            opts="-e -h -V --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__down)
            opts="-e -h -V --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__enable)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help)
            opts="setup up enable disable down status tls services dashboard mounts open help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__dashboard)
            opts="serve"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__dashboard__subcmd__serve)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__down)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__mounts)
            opts="plan configure disable"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__mounts__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__open)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services)
            opts="plan apply status upgrade repair remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__services__subcmd__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__setup)
            opts="project"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__setup__subcmd__project)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__tls)
            opts="status repair rotate uninstall"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__tls__subcmd__uninstall)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__help__subcmd__up)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version plan configure disable help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__configure)
            opts="-e -h -V --source --mount-target --service --preset --read-only --safe-exception --plan --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --source)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mount-target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --service)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --preset)
                    COMPREPLY=($(compgen -W "laravel node dotnet rust" -- "${cur}"))
                    return 0
                    ;;
                --safe-exception)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__disable)
            opts="-e -h -V --plan --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__help)
            opts="plan configure disable help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__mounts__subcmd__plan)
            opts="-e -h -V --source --mount-target --service --preset --read-only --safe-exception --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --source)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mount-target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --service)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --preset)
                    COMPREPLY=($(compgen -W "laravel node dotnet rust" -- "${cur}"))
                    return 0
                    ;;
                --safe-exception)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__open)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version plan apply status upgrade repair remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__apply)
            opts="-e -h -V --plan --smtp-port --gate --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --smtp-port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help)
            opts="plan apply status upgrade repair remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__help__subcmd__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__plan)
            opts="-e -h -V --operation --smtp-port --purge --gate --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --operation)
                    COMPREPLY=($(compgen -W "apply upgrade repair remove" -- "${cur}"))
                    return 0
                    ;;
                --smtp-port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__remove)
            opts="-e -h -V --plan --purge --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__repair)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__services__subcmd__upgrade)
            opts="-e -h -V --plan --smtp-port --gate --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --smtp-port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__setup)
            opts="-e -h -V --no-trust --system --services --smtp-port --yes --root --environment --runtime --target --mm-home --output --help --version project help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --smtp-port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__setup__subcmd__help)
            opts="project help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__setup__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__setup__subcmd__help__subcmd__project)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__setup__subcmd__project)
            opts="-e -h -V --plan --deploy-plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --deploy-plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls)
            opts="-e -h -V --system --root --environment --runtime --target --mm-home --output --help --version status repair rotate uninstall help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help)
            opts="status repair rotate uninstall help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__help__subcmd__uninstall)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__repair)
            opts="-e -h -V --system --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__rotate)
            opts="-e -h -V --system --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__status)
            opts="-e -h -V --system --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__tls__subcmd__uninstall)
            opts="-e -h -V --system --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__dev__subcmd__up)
            opts="-e -h -V --with --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --with)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__doctor)
            opts="-e -h -V --check-updates --root --environment --runtime --target --mm-home --output --help --version recover-state help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__doctor__subcmd__help)
            opts="recover-state help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__doctor__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__doctor__subcmd__help__subcmd__recover__subcmd__state)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__doctor__subcmd__recover__subcmd__state)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status explain set unset discard default check materialize diff review setup delivery help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__check)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version show set unset help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__help)
            opts="show set unset help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__help__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__help__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__set)
            opts="-h -V --root --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__default__subcmd__unset)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version show set help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__help)
            opts="show set help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__set)
            opts="-h -V --root --runtime --target --mm-home --output --help --version direct env-file"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__delivery__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__diff)
            opts="-e -h -V --service --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --service)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__discard)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__explain)
            opts="-e -h -V --no-check-secrets --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help)
            opts="status explain set unset discard default check materialize diff review setup delivery help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__check)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__default)
            opts="show set unset"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__default__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__default__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__default__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__delivery)
            opts="show set"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__delivery__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__delivery__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__diff)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__discard)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__materialize)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__review)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__help__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__materialize)
            opts="-e -h -V --service --force --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --service)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__review)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__set)
            opts="-e -h -V --stdin --expected-revision --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --expected-revision)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__setup)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__status)
            opts="-e -h -V --no-check-secrets --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__env__subcmd__unset)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward)
            opts="-e -h -V --local --bind --root --environment --runtime --target --mm-home --output --help --version save list remove up help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --local)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bind)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help)
            opts="save list remove up help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help__subcmd__save)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__help__subcmd__up)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__remove)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__save)
            opts="-e -h -V --local --bind --auto --root --environment --runtime --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --local)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --bind)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__forward__subcmd__up)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help)
            opts="init dev config doctor ingress service job env secret access build deploy team forward key target agent history workspace update mcp help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access)
            opts="add remove show attach detach user session"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__attach)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__detach)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__session)
            opts="list revoke rotate-key"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__session__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__session__subcmd__revoke)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__session__subcmd__rotate__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user)
            opts="add disable enable password-reset remove list"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__password__subcmd__reset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__access__subcmd__user__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__agent)
            opts="status trust setup"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__agent__subcmd__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__agent__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__agent__subcmd__trust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__build)
            opts="plan apply receipt import"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__build__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__build__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__build__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__build__subcmd__receipt)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config)
            opts="validate module service stack build environment binding secret ingress mount job schedule access user schema split join sources"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__access)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__access__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__access__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__access__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__binding)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__binding__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__build)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__build__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__build__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__build__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__environment)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__environment__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress)
            opts="add edit remove list explain"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__ingress__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__job)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__job__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__job__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__job__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__join)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__module)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__module__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__module__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__module__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__mount)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__mount__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__schedule)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__schedule__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__schema)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__secret)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__secret__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__service)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__service__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__service__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__service__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__sources)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__split)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__stack)
            opts="add edit remove compile"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__compile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__stack__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__user)
            opts="add edit remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__user__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__user__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__user__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__config__subcmd__validate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy)
            opts="plan apply rollback redeploy status logs cancel"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__cancel)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__logs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__redeploy)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__rollback)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__deploy__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev)
            opts="setup up enable disable down status tls services dashboard mounts open"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__dashboard)
            opts="serve"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__dashboard__subcmd__serve)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__down)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__mounts)
            opts="plan configure disable"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__mounts__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__open)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services)
            opts="plan apply status upgrade repair remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__services__subcmd__upgrade)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__setup)
            opts="project"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__setup__subcmd__project)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__tls)
            opts="status repair rotate uninstall"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__repair)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__tls__subcmd__uninstall)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__dev__subcmd__up)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__doctor)
            opts="recover-state"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__doctor__subcmd__recover__subcmd__state)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env)
            opts="status explain set unset discard default check materialize diff review setup delivery"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__check)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__default)
            opts="show set unset"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__default__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__default__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__default__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__delivery)
            opts="show set"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__delivery__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__delivery__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__diff)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__discard)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__explain)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__materialize)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__review)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__setup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__env__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__forward)
            opts="save list remove up"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__forward__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__forward__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__forward__subcmd__save)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__forward__subcmd__up)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__history)
            opts="show"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__history__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__ingress)
            opts="start stop status restart"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__ingress__subcmd__restart)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__ingress__subcmd__start)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__ingress__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__ingress__subcmd__stop)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job)
            opts="list run status history cancel retry schedule"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__cancel)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__history)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__retry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule)
            opts="reconcile list show enable disable run-now next history tick daemon"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__daemon)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__history)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__next)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__reconcile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__run__subcmd__now)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__schedule__subcmd__tick)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__job__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key)
            opts="create show protect rotate remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key__subcmd__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key__subcmd__protect)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__key__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__mcp)
            opts="serve"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__mcp__subcmd__serve)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret)
            opts="providers init status set unset discard generate rotate migrate rotation provider"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__discard)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__generate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__migrate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider)
            opts="list show add edit test status plan apply backup restore snapshot recovery remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__backup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery)
            opts="export import"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__export)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__restore)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__snapshot)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__provider__subcmd__test)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__providers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__rotation)
            opts="plan apply"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__rotation__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__rotation__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__secret__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service)
            opts="status logs exec shell restart"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service__subcmd__exec)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service__subcmd__logs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service__subcmd__restart)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service__subcmd__shell)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__service__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target)
            opts="local list show nodes mesh storage connections trust add update remove select preference setup ingress access"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__access)
            opts="status"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__access__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__connections)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__ingress)
            opts="configure plan"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__ingress__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__ingress__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__local)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__mesh)
            opts="status rotate"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__mesh__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__mesh__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__nodes)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference)
            opts="show set-runtime unset-runtime set-target unset-target set-address unset-address"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__set__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__preference__subcmd__unset__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__select)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__setup)
            opts="plan apply deploy-key record"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__deploy__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__setup__subcmd__record)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__storage)
            opts="status delete"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__storage__subcmd__delete)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__storage__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__trust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__target__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team)
            opts="join add remove show whoami sync"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__join)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__sync)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__team__subcmd__whoami)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__workspace)
            opts="list forget"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__workspace__subcmd__forget)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__help__subcmd__workspace__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__history)
            opts="-e -h -V --outcome --since --limit --before --root --environment --runtime --target --mm-home --output --help --version show help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --outcome)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --since)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --before)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__history__subcmd__help)
            opts="show help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__history__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__history__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__history__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version start stop status restart help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help)
            opts="start stop status restart help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help__subcmd__restart)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help__subcmd__start)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__help__subcmd__stop)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__restart)
            opts="-e -h -V --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__start)
            opts="-e -h -V --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__ingress__subcmd__stop)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__init)
            opts="-e -h -V --name --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version list run status history cancel retry schedule help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__cancel)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help)
            opts="list run status history cancel retry schedule help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__cancel)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__history)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__retry)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__run)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule)
            opts="reconcile list show enable disable run-now next history tick daemon"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__daemon)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__history)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__next)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__reconcile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__run__subcmd__now)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__schedule__subcmd__tick)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__history)
            opts="-e -h -V --limit --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__retry)
            opts="-e -h -V --no-wait --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__run)
            opts="-e -h -V --no-wait --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version reconcile list show enable disable run-now next history tick daemon help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__daemon)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__disable)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__enable)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help)
            opts="reconcile list show enable disable run-now next history tick daemon help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__daemon)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__disable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__enable)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__history)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__next)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__reconcile)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__run__subcmd__now)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__help__subcmd__tick)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__history)
            opts="-e -h -V --limit --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --limit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__next)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__reconcile)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__run__subcmd__now)
            opts="-e -h -V --no-wait --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__schedule__subcmd__tick)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__job__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version create show protect rotate remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__create)
            opts="-e -h -V --security-key --agent --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help)
            opts="create show protect rotate remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__create)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__protect)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__protect)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__remove)
            opts="-e -h -V --agent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__rotate)
            opts="-e -h -V --agent --finish --unlisted --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__key__subcmd__show)
            opts="-e -h -V --agent --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__mcp)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version serve help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__mcp__subcmd__help)
            opts="serve help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__mcp__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__mcp__subcmd__help__subcmd__serve)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__mcp__subcmd__serve)
            opts="-e -h -V --config --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version providers init status set unset discard generate rotate migrate rotation provider help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__discard)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__generate)
            opts="-e -h -V --missing --all --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help)
            opts="providers init status set unset discard generate rotate migrate rotation provider help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__discard)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__generate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__init)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__migrate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider)
            opts="list show add edit test status plan apply backup restore snapshot recovery remove"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__backup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery)
            opts="export import"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery__subcmd__export)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__recovery__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__restore)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__snapshot)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__provider__subcmd__test)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__providers)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__rotation)
            opts="plan apply"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__rotation__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__rotation__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__set)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__help__subcmd__unset)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__init)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version age"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__migrate)
            opts="-e -h -V --from --to --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --from)
                    COMPREPLY=($(compgen -W "age" -- "${cur}"))
                    return 0
                    ;;
                --to)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version list show add edit test status plan apply backup restore snapshot recovery remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__add)
            opts="-e -h -V --driver --mode --scope --bootstrap --stdin --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --driver)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mode)
                    COMPREPLY=($(compgen -W "managed external" -- "${cur}"))
                    return 0
                    ;;
                --scope)
                    COMPREPLY=($(compgen -W "installation project" -- "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__apply)
            opts="-e -h -V --operation --purge --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --operation)
                    COMPREPLY=($(compgen -W "install restart upgrade seal unseal remove" -- "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__backup)
            opts="-e -h -V --file --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__edit)
            opts="-e -h -V --driver --mode --bootstrap --stdin --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --driver)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mode)
                    COMPREPLY=($(compgen -W "managed external" -- "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help)
            opts="list show add edit test status plan apply backup restore snapshot recovery remove help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__backup)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery)
            opts="export import"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery__subcmd__export)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__recovery__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__restore)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__snapshot)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__help__subcmd__test)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__plan)
            opts="-e -h -V --operation --purge --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --operation)
                    COMPREPLY=($(compgen -W "install restart upgrade seal unseal remove" -- "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version export import help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__export)
            opts="-e -h -V --recipient --to --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --recipient)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --to)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help)
            opts="export import help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__export)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__help__subcmd__import)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 6 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__recovery__subcmd__import)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__remove)
            opts="-e -h -V --plan --yes --purge --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__restore)
            opts="-e -h -V --file --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__snapshot)
            opts="-e -h -V --to --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --to)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__provider__subcmd__test)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__providers)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotate)
            opts="-e -h -V --stdin --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version plan apply help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__apply)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__help)
            opts="plan apply help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__rotation__subcmd__plan)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__set)
            opts="-e -h -V --stdin --local --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__status)
            opts="-e -h -V --local --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__secret__subcmd__unset)
            opts="-e -h -V --yes --local --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status logs exec shell restart help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__exec)
            opts="-e -h -V --stack --container --user --workdir --env --tty --stdin --timeout --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --stack)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --container)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --user)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --workdir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --env)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --timeout)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help)
            opts="status logs exec shell restart help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__exec)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__logs)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__restart)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__shell)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__logs)
            opts="-e -h -V --stack --node --follow --tail --since --until --timestamps --stdout --stderr --max-line-bytes --max-bytes-per-second --max-lines-per-second --raw --approve-high-risk --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --stack)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --node)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --tail)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --since)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --until)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-line-bytes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-bytes-per-second)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-lines-per-second)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__restart)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__shell)
            opts="-e -h -V --stack --container --user --workdir --approve-high-risk --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --stack)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --container)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --user)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --workdir)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__service__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version local list show nodes mesh storage connections trust add update remove select preference setup ingress access help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__access)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__access__subcmd__help)
            opts="status help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__access__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__access__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__access__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__add)
            opts="-e -h -V --host --user --port --host-key-fingerprint --host-key --identity-file --remote-root --daemon-fingerprint --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --host)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --user)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-key-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --identity-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --remote-root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --daemon-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__connections)
            opts="-e -h -V --close --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help)
            opts="local list show nodes mesh storage connections trust add update remove select preference setup ingress access help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__access)
            opts="status"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__access__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__connections)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__ingress)
            opts="configure plan"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__ingress__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__ingress__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__local)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__mesh)
            opts="status rotate"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__mesh__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__mesh__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__nodes)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference)
            opts="show set-runtime unset-runtime set-target unset-target set-address unset-address"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__set__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__preference__subcmd__unset__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__select)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__setup)
            opts="plan apply deploy-key record"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__deploy__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__setup__subcmd__record)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__storage)
            opts="status delete"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__storage__subcmd__delete)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__storage__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__trust)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__help__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version configure plan help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__configure)
            opts="-e -h -V --image --acme-email --acme-ca-server --acme-ca-certificate --expected-fingerprint --root --environment --runtime --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --image)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --acme-email)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --acme-ca-server)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --acme-ca-certificate)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --expected-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__help)
            opts="configure plan help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__configure)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__ingress__subcmd__plan)
            opts="-e -h -V --root --environment --runtime --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__local)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status rotate help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__help)
            opts="status rotate help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__rotate)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__rotate)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__mesh__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__nodes)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version show set-runtime unset-runtime set-target unset-target set-address unset-address help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help)
            opts="show set-runtime unset-runtime set-target unset-target set-address unset-address help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__set__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__address)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__runtime)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__help__subcmd__unset__subcmd__target)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__address)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__runtime)
            opts="-h -V --environment --root --runtime --target --mm-home --output --help --version podman-compose docker-compose"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__set__subcmd__target)
            opts="-h -V --environment --root --runtime --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__address)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__runtime)
            opts="-h -V --environment --root --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__preference__subcmd__unset__subcmd__target)
            opts="-h -V --environment --root --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__remove)
            opts="-e -h -V --expected-fingerprint --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --expected-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__select)
            opts="-h -V --environment --root --runtime --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version plan apply deploy-key record help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__apply)
            opts="-e -h -V --plan --yes --origin --delivery --origin-host-key --break-glass --host-binaries --trust-new-host-keys --commit --nodes --downgrade --upgrade --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --origin)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --delivery)
                    COMPREPLY=($(compgen -W "push" -- "${cur}"))
                    return 0
                    ;;
                --origin-host-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --break-glass)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-binaries)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --commit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__deploy__subcmd__key)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help)
            opts="plan apply deploy-key record help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__apply)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__deploy__subcmd__key)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__plan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__help__subcmd__record)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__plan)
            opts="-e -h -V --origin --delivery --origin-host-key --break-glass --host-binaries --trust-new-host-keys --commit --nodes --downgrade --upgrade --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --origin)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --delivery)
                    COMPREPLY=($(compgen -W "push" -- "${cur}"))
                    return 0
                    ;;
                --origin-host-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --break-glass)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-binaries)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --commit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__setup__subcmd__record)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version status delete help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__delete)
            opts="-e -h -V --stack --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --stack)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__help)
            opts="status delete help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__delete)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__help__subcmd__status)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__storage__subcmd__status)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__trust)
            opts="-e -h -V --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__target__subcmd__update)
            opts="-e -h -V --host --user --port --host-key-fingerprint --host-key --identity-file --remote-root --daemon-fingerprint --expected-fingerprint --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --host)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --user)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-key-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --host-key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --identity-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --remote-root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --daemon-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --expected-fingerprint)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version join add remove show whoami sync help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__add)
            opts="-e -h -V --key --machine --access --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --key)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --access)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help)
            opts="join add remove show whoami sync help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__add)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__join)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__remove)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__sync)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__help__subcmd__whoami)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__join)
            opts="-e -h -V --name --request --agent --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --name)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --request)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__remove)
            opts="-e -h -V --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__show)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__sync)
            opts="-e -h -V --commit --nodes --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --commit)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --nodes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__team__subcmd__whoami)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__update)
            opts="-e -h --check --version --force --yes --root --environment --runtime --target --mm-home --output --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --version)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version list forget help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__forget)
            opts="-e -h -V --stale --plan --yes --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --plan)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__help)
            opts="list forget help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__help__subcmd__forget)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__help__subcmd__list)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        mm__subcmd__workspace__subcmd__list)
            opts="-e -h -V --root --environment --runtime --target --mm-home --output --help --version"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --root)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --environment)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -e)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --runtime)
                    COMPREPLY=($(compgen -W "podman-compose docker-compose" -- "${cur}"))
                    return 0
                    ;;
                --target)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --mm-home)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --output)
                    COMPREPLY=($(compgen -W "human json" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _mm -o nosort -o bashdefault -o default mm
else
    complete -F _mm -o bashdefault -o default mm
fi
