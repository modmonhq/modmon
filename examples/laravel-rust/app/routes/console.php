<?php

use Illuminate\Support\Facades\Schedule;

Schedule::call(
    static fn () => logger()->info('Example scheduler heartbeat')
)->hourly()->name('example-heartbeat');
