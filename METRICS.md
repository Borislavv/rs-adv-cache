## Main cache

	AvgTotalDuration         = "avg_duration_ns"
	AvgCacheDuration         = "avg_cache_duration_ns"
	AvgProxyDuration         = "avg_proxy_duration_ns"
	AvgErrorDuration         = "avg_error_duration_ns"

	RPS                      = "rps"
	Total                    = "total" 
	Errored                  = "errors"  
	Panicked                 = "panics"  
	Proxied                  = "proxies"
	Hits                     = "cache_hits"
	Misses                   = "cache_misses"
	MapMemoryUsageMetricName = "cache_memory_usage"
	MapLength                = "cache_length"

	TotalSoftEvictions       = "soft_evicted_total_items"
	TotalSoftBytesEvicted    = "soft_evicted_total_bytes"
	TotalSoftScans           = "soft_evicted_total_scans"

	TotalHardEvictions       = "hard_evicted_total_items"
	TotalHardBytesEvicted    = "hard_evicted_total_bytes"

	TotalAdmAllowed          = "admission_allowed"
	TotalAdmNotAllowed       = "admission_not_allowed"

	RefresherUpdated         = "refresh_updated"
	RefresherErrors          = "refresh_errors"
	RefresherScans           = "refresh_scans"
	RefresherHits            = "refresh_hits"
	RefresherMiss            = "refresh_miss"

    BackendPolicy            = "backend_policy"
	LifetimePolicy           = "lifetime_policy"

	IsBypassActive           = "is_bypass_active"
	IsCompressionActive      = "is_compression_active"
	IsTracesActive           = "is_traces_active"
	IsAdmissionActive        = "is_admission_active"