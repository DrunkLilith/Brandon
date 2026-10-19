//! Original starter content, not imported Miro/Obsidian assets or plugin code.
use super::project_templates::{Template, TemplateFile};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn file(path: &str, content: String) -> TemplateFile {
    TemplateFile {
        path: path.into(),
        content,
        extra: BTreeMap::new(),
    }
}
fn task_views(first: &str) -> String {
    let order = std::iter::once(first).chain(
        ["table", "kanban", "timeline"]
            .into_iter()
            .filter(|v| *v != first),
    );
    let views:Vec<_>=order.map(|layout|json!({"id":layout,"name":match layout{"table"=>"Таблица","kanban"=>"Kanban",_=>"Timeline"},"type":format!("brandon-task-{layout}"),"windowDays":14})).collect();
    serde_json::to_string_pretty(&json!({"brandon":{"version":1,"format":"task-view","filter":{"project":"{{root}}"}},"views":views})).expect("own task format")+"\n"
}
fn text(
    id: &str,
    [x, y, width, height]: [i32; 4],
    label: &str,
    color: &str,
    primitive: &str,
) -> Value {
    json!({"id":id,"type":"text","x":x,"y":y,"width":width,"height":height,"text":label,"color":color,"brandon":{"version":1,"primitive":primitive}})
}
fn card(id: &str, path: &str, x: i32, y: i32, width: i32, height: i32) -> Value {
    json!({"id":id,"type":"file","file":format!("{{{{root}}}}/{path}"),"x":x,"y":y,"width":width,"height":height})
}
fn starter(
    [id, name, category, description, note_name]: [&str; 5],
    note: &str,
    first: &str,
    columns: [&str; 3],
    architecture: bool,
) -> String {
    let mut nodes = vec![
        json!({"id":"workspace","type":"group","label":"{{project}}","x":0,"y":0,"width":1440,"height":1000,"brandon":{"version":1,"frame":{"version":1,"preset":"desktop","notes":"Сначала обозначьте цель и источники. После обсуждения создайте настоящие задачи в связанном представлении; статусы не дублируются стикерами."}}}),
    ];
    for (i, label) in columns.into_iter().enumerate() {
        nodes.push(text(
            &format!("focus-{i}"),
            [60 + i as i32 * 450, 90, 420, 220],
            label,
            ["6", "5", "3"][i],
            if architecture { "rectangle" } else { "sticky" },
        ))
    }
    nodes.push(card("notes", note_name, 60, 370, 610, 350));
    nodes.push(card("tasks", "Tasks.base", 730, 370, 650, 350));
    nodes.push(text("guide",[60,780,1320,150],"Один источник — несколько представлений\nОткройте документ или .base кнопкой «Показать / править здесь». Статусы задач хранятся в общей базе, описание — в Markdown. Дата плана и дедлайн независимы.","6","text"));
    let edges = if architecture {
        vec![
            json!({"id":"input-core","fromNode":"focus-0","toNode":"focus-1","fromSide":"right","toSide":"left","label":"контракт"}),
            json!({"id":"core-store","fromNode":"focus-1","toNode":"focus-2","fromSide":"right","toSide":"left","label":"данные"}),
        ]
    } else {
        Vec::new()
    };
    let board = json!({"nodes":nodes,"edges":edges,"brandon":{"version":1,"presentation":{"version":1,"frames":["workspace"]}}});
    let template = Template {
        format: "brandon.project-template".into(),
        version: 1,
        id: format!("builtin-{id}"),
        name: name.into(),
        category: category.into(),
        description: description.into(),
        entrypoint: "Workspace.canvas".into(),
        files: vec![
            file(
                note_name,
                format!(
                    "# {{{{project}}}}\n\nДата: {{{{date}}}}\n\n{note}\n\n## Связанные материалы\n\n[Рабочая доска](Workspace.canvas) · [Общие задачи проекта](Tasks.base)\n\nКлюч проекта: `{{{{root}}}}`. Это фильтр общей базы задач, не отдельная копия статусов.\n"
                ),
            ),
            file("Tasks.base", task_views(first)),
            file(
                "Workspace.canvas",
                serde_json::to_string_pretty(&board).expect("own canvas") + "\n",
            ),
        ],
        extra: BTreeMap::new(),
    };
    serde_json::to_string_pretty(&template).expect("own starter") + "\n"
}
pub(super) fn sources() -> Vec<String> {
    vec![
        starter(
            [
                "project-workspace",
                "Проектное пространство",
                "Проекты",
                "Цель, решения, источники, связанная таблица/Kanban/Timeline и один фрейм для обзора.",
                "Overview.md",
            ],
            "## Зачем\n\nКакой результат нужен и кому?\n\n## Границы и критерии готовности\n\nЧто входит в проект, что сознательно оставляем снаружи?\n\n## Решения\n\nФакт → варианты → выбранное решение → причина.\n\n## Следующий шаг\n\nСоздайте настоящую задачу в разделе задач и укажите ключ этого проекта.",
            "table",
            [
                "Цель\nКакой результат нужен?",
                "Решения\nНа чём основываем выбор?",
                "Следующий шаг\nЧто проверим первым?",
            ],
            false,
        ),
        starter(
            [
                "research-board",
                "Исследовательская доска",
                "Исследования",
                "Вопрос → наблюдения и источники → проверяемый следующий эксперимент.",
                "Research.md",
            ],
            "## Исследовательский вопрос\n\nЧто хотим узнать?\n\n## Исходные предположения\n\nКак отличим факт от гипотезы?\n\n## Источники и наблюдения\n\nИсточник, точное наблюдение, ограничения и противоречия.\n\n## Эксперимент\n\nЧто измеряем, какой результат опровергнет гипотезу?\n\n## Вывод\n\nУверенность и оставшиеся вопросы.",
            "table",
            [
                "Вопрос\nСформулируйте гипотезу",
                "Доказательства\nФакты и ограничения",
                "Эксперимент\nКритерий проверки",
            ],
            false,
        ),
        starter(
            [
                "architecture-map",
                "Карта архитектуры",
                "Проектирование",
                "Три связанных компонента, явные контракты, журнал решений и общие задачи.",
                "Architecture.md",
            ],
            "## Контекст и границы\n\nПользователи, внешние системы, ресурсы.\n\n## Компоненты и контракты\n\nВходы, выходы, владельцы данных, ошибки.\n\n## Ограничения\n\nНагрузка, безопасность, отказоустойчивость и переносимость.\n\n## Решения и альтернативы\n\nПочему этот вариант? Как проверить?\n\n## Риски\n\nЧто может сломаться и как обнаружим?",
            "table",
            [
                "Вход / клиент\nГраница доверия",
                "Обработка\nКонтракт и ошибки",
                "Хранилище\nВладелец и восстановление",
            ],
            true,
        ),
        starter(
            [
                "kanban",
                "Kanban проекта",
                "Планирование",
                "Настоящий связанный Kanban общей базы, а не статические колонки с копиями задач.",
                "Workflow.md",
            ],
            "## Результат и приоритет\n\nКакую работу сейчас ведём?\n\n## Политика потока\n\nК выполнению → В работе → Готово.\n\n## Ограничение незавершённой работы\n\nСколько задач берём одновременно?\n\n## Готово — это\n\nКакие проверки должны пройти?\n\nПеренос карточки в Kanban меняет status одной TaskId. Плановая дата и дедлайн не привязаны к колонке.",
            "kanban",
            [
                "Приоритет\nЧто важно сейчас?",
                "Ограничение WIP\nСколько берём в работу?",
                "Готово\nКак проверяем результат?",
            ],
            false,
        ),
        starter(
            [
                "roadmap",
                "Дорожная карта",
                "Планирование",
                "Связанный Timeline с независимыми планом, длительностью и дедлайном, плюс риски и этапы.",
                "Roadmap.md",
            ],
            "## Желаемый результат\n\nКак поймём, что движение полезно?\n\n## Этапы\n\nЭтап → ожидаемый результат → проверка.\n\n## Зависимости\n\nЧто должно произойти раньше?\n\n## Риски и допущения\n\nЧто может изменить план?\n\n## Даты\n\nПлан — когда работаем. Длительность — объём интервала. Дедлайн — отдельное ограничение. Не превращайте дату без времени в выдуманную встречу.",
            "timeline",
            [
                "Ближайший результат\nПроверяемая ценность",
                "Зависимости\nЧто нужно раньше?",
                "Риски\nКогда пересматриваем план?",
            ],
            false,
        ),
        starter(
            [
                "retrospective",
                "Ретроспектива",
                "Команда",
                "Три зоны обратной связи, решения с причиной и связанные действия без копирования задач.",
                "Retrospective.md",
            ],
            "## Контекст\n\nПериод, участники, цель обсуждения.\n\n## Что помогло\n\nКонкретные события, не общие оценки.\n\n## Что мешало\n\nНаблюдения без поиска виноватого.\n\n## Что попробуем\n\nОдно проверяемое изменение, владелец и способ оценки.\n\n## Действия\n\nПосле выбора создайте задачи в общей базе с ключом проекта.",
            "kanban",
            [
                "Помогло\nЧто стоит сохранить?",
                "Мешало\nКонкретные наблюдения",
                "Попробуем\nОдна проверяемая перемена",
            ],
            false,
        ),
        starter(
            [
                "meeting-notes",
                "Встреча и решения",
                "Документы",
                "Повестка, факты, решения и последующие действия; рабочая доска связывает исходные заметки и задачи.",
                "Meeting.md",
            ],
            "## Цель встречи\n\nКакой результат нужен к концу?\n\n## Участники\n\nКто принимает решение и кто исполняет?\n\n## Повестка\n\n1. Контекст\n2. Варианты\n3. Решение\n\n## Заметки\n\nФакты отдельно от предположений.\n\n## Решения\n\nРешение → причина → последствия.\n\n## Действия\n\nСоздайте TaskId, укажите проект и отдельно договоритесь о плане/дедлайне.",
            "table",
            [
                "Повестка\nЧто обсуждаем?",
                "Решения\nЧто выбрали и почему?",
                "Действия\nКто и что делает?",
            ],
            false,
        ),
        starter(
            [
                "daily-planning",
                "Дневной план",
                "Личное",
                "Фокус дня, заметка и связанные виды задач; дата плана не подменяет дедлайн.",
                "Daily.md",
            ],
            "## Фокус дня\n\nКакой один результат важнее всего?\n\n## Ограничения\n\nДоступное время, встречи, обязательства.\n\n## План\n\nВ связанном представлении задайте плановую дату реальным задачам. Время необязательно; длительность задаётся отдельно.\n\n## Быстрые заметки\n\nМысли и наблюдения.\n\n## Итог дня\n\nЧто сделано, чему научились, что перенести?",
            "timeline",
            [
                "Фокус\nОдин главный результат",
                "Ограничения\nВремя и обязательства",
                "Итог\nЧто изменилось за день?",
            ],
            false,
        ),
    ]
}
