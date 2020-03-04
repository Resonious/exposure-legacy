# frozen_string_literal: true

module Exposure
  # Light wrapper around the Core Trace
  class Trace
    def initialize
      @core = Core.new_trace

      @trace_points = []
      @trace_points << TracePoint.new(:b_call, :class, :call, &method(:push))
      @trace_points << TracePoint.new(:return, :b_return, :end, &method(:pop))
    end

    def start
      @trace_points.each(&:enable)
    end

    def stop
      @trace_points.each(&:disable)
    end

    def push(trace)
      calla = caller_locations(2..2).first
      receiver = trace.binding.receiver
      klass = trace.defined_class

      # First push
      Core.push_frame(
        @core,

        trace.event,

        calla.path,
        calla.lineno,

        trace.path,
        trace.lineno,

        klass.name || klass.to_s,
        trace.method_id,

        receiver.name || receiver.to_s
      )

      # Then add locals
      trace.binding.local_variables.each do |var|
        begin
          val = trace.binding.local_variable_get(var)
          Core.add_local(@core, var, val.class.name || val.class.to_s)
        rescue StandardError => e
          Core.add_local(@core, var, "((#{e.class} during inspect))")
        end
      end
    end

    def pop(trace)
      if %i[return b_return].include?(trace.event)
        return_type = Frame.sanitized_class_name(trace.return_value.class.name)
      else
        return_type = ''
      end

      Core.pop_frame(@core, return_type)
    end
  end
end
