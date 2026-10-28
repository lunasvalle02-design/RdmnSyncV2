# Datos de prueba. Variables: ACTIVITIES, PROJECT_NAME, PROJECT_ID, ISSUES (separados por |),
# ENTRIES ("índice_issue;día_semana;horas;actividad;comentario" separados por |, índice -1 = sin issue).
Setting.rest_api_enabled = '1'
Setting.default_language = 'es'
Redmine::DefaultData::Loader.load('es') unless Tracker.any?

admin = User.find_by_login('admin')
admin.update_columns(must_change_passwd: false)

names = ENV.fetch('ACTIVITIES').split(',')
TimeEntryActivity.where.not(name: names).update_all(active: false)
names.each_with_index do |n, i|
  a = TimeEntryActivity.find_or_initialize_by(name: n)
  a.update!(active: true, position: i + 1, is_default: i.zero?)
end

project = Project.find_by(identifier: ENV.fetch('PROJECT_ID')) || Project.create!(
  name: ENV.fetch('PROJECT_NAME'), identifier: ENV.fetch('PROJECT_ID'), is_public: false,
  enabled_module_names: %w[issue_tracking time_tracking], trackers: Tracker.all
)
role = Role.givable.first
Member.create!(project: project, principal: admin, roles: [role]) unless Member.where(project: project, user_id: admin.id).exists?

issues = ENV.fetch('ISSUES', '').split('|').map do |subject|
  Issue.find_by(project: project, subject: subject) || Issue.create!(
    project: project, tracker: Tracker.first, subject: subject, author: admin, assigned_to: admin,
    priority: IssuePriority.default || IssuePriority.first, description: "Descripción de «#{subject}»",
    estimated_hours: 8
  )
end

ENV.fetch('VERSIONS', '').split(',').each { |n| Version.find_or_create_by!(project: project, name: n) }

if (rename = ENV['STATUS_RENAME'])
  from, to = rename.split('=')
  IssueStatus.find_by(name: from)&.update!(name: to)
end

# "índice;estado;%;versión" separados por |
ENV.fetch('ISSUE_STATE', '').split('|').each do |spec|
  idx, status, done, version = spec.split(';')
  issue = issues[idx.to_i]
  issue.status = IssueStatus.find_by!(name: status)
  issue.done_ratio = done.to_i
  issue.fixed_version = Version.find_by(project: project, name: version) if version
  issue.save!
end

monday = Date.today.beginning_of_week
ENV.fetch('ENTRIES', '').split('|').each do |spec|
  idx, day, hours, activity, comment = spec.split(';')
  attrs = { project: project, user: admin, author: admin, spent_on: monday + day.to_i, hours: hours.to_f,
            activity: TimeEntryActivity.find_by!(name: activity), comments: comment }
  attrs[:issue] = issues[idx.to_i] if idx.to_i >= 0
  TimeEntry.create!(attrs) unless TimeEntry.where(spent_on: attrs[:spent_on], hours: attrs[:hours], comments: comment).exists?
end

puts "API_KEY=#{admin.api_key}"
