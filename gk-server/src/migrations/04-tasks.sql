BEGIN;
-- These tasks are created automatically whenever a new item is created of the type that the task is for
CREATE TABLE AutoTask (
    auto_task_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}',
    item_type TEXT NOT NULL,
    created_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    active BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE TABLE AutoTaskDependsOn (
    auto_task_id INTEGER NOT NULL,
    depends_on_auto_task_id INTEGER NOT NULL,
    FOREIGN KEY (auto_task_id) REFERENCES AutoTask(auto_task_id),
    FOREIGN KEY (depends_on_auto_task_id) REFERENCES AutoTask(auto_task_id),
    PRIMARY KEY (auto_task_id, depends_on_auto_task_id)
);

CREATE TABLE Attempt (
    attempt_id INTEGER PRIMARY KEY,
    auto_task_id INTEGER NOT NULL,
    item_id INTEGER NOT NULL,
    started_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_on TIMESTAMP,
    success BOOLEAN,
    FOREIGN KEY (auto_task_id) REFERENCES AutoTask(auto_task_id)
);


CREATE VIEW Task AS
WITH should_have_attempts AS (
    SELECT
        AutoTask.*,
        Recipe.recipe_id AS item_id
    FROM AutoTask
    CROSS JOIN Recipe
    WHERE AutoTask.item_type = 'recipe'
    UNION ALL
    SELECT
        AutoTask.*,
        Revision.revision_id AS item_id
    FROM AutoTask
    CROSS JOIN Revision
    WHERE AutoTask.item_type = 'revision'
),
task_attempt_stats AS (
    SELECT
        attempt.auto_task_id,
        attempt.item_id,
        max(coalesce(success, 0)) AS success,
        max(attempt.started_on) AS last_attempt_started_on,
        count(attempt.attempt_id) AS attempt_count
    FROM Attempt
    GROUP BY attempt.auto_task_id, attempt.item_id
),
all_dependencies_succeeded AS (
    SELECT
        atask.auto_task_id,
        atask.item_id,
        prereq.depends_on_auto_task_id IS NULL
            OR coalesce(min(task_attempt_stats.success), 0)
            AS ready
    FROM should_have_attempts AS atask
    LEFT JOIN AutoTaskDependsOn prereq
        ON atask.auto_task_id = prereq.auto_task_id
    LEFT JOIN task_attempt_stats
        ON prereq.depends_on_auto_task_id = task_attempt_stats.auto_task_id
        AND atask.item_id = task_attempt_stats.item_id
    GROUP BY atask.auto_task_id, atask.item_id
)
SELECT
    atask.*,
    task_attempt_stats.attempt_count,
    task_attempt_stats.success,
    task_attempt_stats.last_attempt_started_on,
    all_dependencies_succeeded.ready
FROM should_have_attempts AS atask
LEFT JOIN all_dependencies_succeeded
    ON atask.auto_task_id = all_dependencies_succeeded.auto_task_id
    AND atask.item_id = all_dependencies_succeeded.item_id
LEFT JOIN task_attempt_stats
    ON atask.auto_task_id = task_attempt_stats.auto_task_id
    AND atask.item_id = task_attempt_stats.item_id;

UPDATE Metadata SET value = 4 WHERE key = 'schema_version';
COMMIT;
