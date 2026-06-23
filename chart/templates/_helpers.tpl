{{- define "ldapmon.fullname" -}}{{ .Release.Name }}{{- end }}

{{- define "ldapmon.selectorLabels" -}}
app.kubernetes.io/name: ldapmon
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "ldapmon.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version }}
{{ include "ldapmon.selectorLabels" . }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "ldapmon.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}{{ default .Release.Name .Values.serviceAccount.name }}{{- else }}default{{- end }}
{{- end }}

{{- define "ldapmon.configmapName" -}}{{ .Release.Name }}-config{{- end }}
