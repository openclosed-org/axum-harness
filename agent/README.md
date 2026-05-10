# Agent Harness

`agent/` 只保留最小的 agent 协作控制面，不承载业务逻辑、完整系统模型或形式化证明。

## 目录职责

1. `codemap.yml`
   负责目录边界、写权限、依赖方向、生成物只读位置、反模式与修改顺序。
2. `manifests/routing-rules.yml`
   负责 touched paths 到 subagent 的路由与派发顺序。
3. `manifests/gate-matrix.yml`
    负责按 changed paths、risk category、evidence level 选择 advisory、guardrail、invariant gates。
4. `brief-format.md`
   负责 durable agent brief 的最小任务契约格式。
5. `skill-authoring.md`
   负责 repository skill 的触发、边界和审查规则。

## 使用顺序

1. 先读根级 `AGENTS.md`。
2. 再读 `agent/codemap.yml`，确认路径边界、导航地图与禁止事项。
3. 最后根据 `routing-rules.yml` 和 `gate-matrix.yml` 决定派发与验证。
4. 只有任务直接需要架构 doctrine、ADR、runbook 或 template guidance 时，才读取对应 `docs/**`。

## 读取模型

1. 全局规则和报告要求：`AGENTS.md`。
2. 路径 ownership、依赖方向和生成物边界：`agent/codemap.yml`。
3. 派发顺序：`agent/manifests/routing-rules.yml`。
4. gate 选择：`agent/manifests/gate-matrix.yml`。
5. skill 行为：相关 `.agents/skills/**/SKILL.md`。
6. 当前行为：代码、schemas、validators、tests、gates、scripts 和命令输出。

不要默认读取全部文档。只有当任务跨边界、触及 durable decision、需要 operator runbook，或用户明确指定时，才扩展读取范围。

## 说明

1. 详细 subagent 行为定义仍在 `.agents/skills/*/SKILL.md`。
2. 参考实现与真实开发模式优先从现有 `services/*`、`workers/*`、`servers/*` 和 `packages/contracts/*` 获取。
3. 如果 `agent/` 文档与代码冲突，以代码和可执行验证结果为准。
4. YAML 只能声明 intent 或 summary；不能单独证明系统语义正确。
5. `docs/_local/**` 是 scratch space，不能作为默认规则来源。
