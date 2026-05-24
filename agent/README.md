# Agent Harness

`agent/` 只保留最小的 agent 协作控制面，不承载业务逻辑、完整系统模型或形式化证明。

## 目录职责

1. `codemap.yml`
   负责目录边界、写权限、依赖方向、生成物只读位置、反模式与修改顺序。
2. `architecture/*.yml`
   负责长期仓库本体论、目录语法、命名规则、gate taxonomy、evidence taxonomy 与熵增防线。
3. `task-profiles/*.yml`
   负责任务态协议，例如重构、迁移、删除、拓扑变更等只在对应任务中启用的 workflow。
4. `manifests/routing-rules.yml`
   负责 touched paths 到 subagent 的路由与派发顺序。
5. `manifests/gate-matrix.yml`
   负责按 changed paths、risk category、evidence level 选择 advisory、guardrail、invariant gates。
6. `brief-format.md`
   负责 durable agent brief 的最小任务契约格式。
7. `skill-authoring.md`
   负责 repository skill 的触发、边界和审查规则。

## 使用顺序

1. 先读根级 `AGENTS.md`。
2. 再读 `agent/codemap.yml`，确认路径边界、导航地图与禁止事项。
3. 结构、命名、所有权边界、重构或控制面任务再读 `agent/architecture/*.yml`。
4. rename、move、split、merge、delete、archive、boundary migration 或 workspace reshaping 任务再读对应 `agent/task-profiles/*.yml`。
5. 最后根据 `routing-rules.yml` 和 `gate-matrix.yml` 决定派发与验证。
6. 只有任务直接需要架构 doctrine、ADR、runbook 或 template guidance 时，才读取对应 `docs/**`。

## 读取模型

1. 全局规则和报告要求：`AGENTS.md`。
2. 路径 ownership、依赖方向和生成物边界：`agent/codemap.yml`。
3. 长期本体论、目录语法、命名规则、gate/evidence taxonomy 和熵增防线：`agent/architecture/*.yml`。
4. 任务态协议：相关 `agent/task-profiles/*.yml`。
5. 派发顺序：`agent/manifests/routing-rules.yml`。
6. gate 选择：`agent/manifests/gate-matrix.yml`。
7. skill 行为：相关 `.agents/skills/**/SKILL.md`。
8. 当前行为：代码、schemas、validators、tests、gates、scripts 和命令输出。

不要默认读取全部文档。只有当任务跨边界、触及 durable decision、需要 operator runbook，或用户明确指定时，才扩展读取范围。

## 说明

1. 详细 subagent 行为定义仍在 `.agents/skills/*/SKILL.md`。
2. 参考实现与真实开发模式优先从现有 `services/*`、`workers/*`、`servers/*` 和 `packages/contracts/*` 获取。
3. 如果 `agent/` 文档与代码冲突，以代码和可执行验证结果为准。
4. YAML 只能声明 intent 或 summary；不能单独证明系统语义正确。
5. `docs/_local/**` 是 scratch space，不能作为默认规则来源。
6. `agent/architecture/*.yml` 是机器可读控制规则，不是业务当前状态证明。
7. `agent/task-profiles/*.yml` 只在对应任务类型中启用，不是所有任务的默认负担。
